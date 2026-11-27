use chrono::{NaiveDate, Utc};
use saispace_core::git_history::REPO_ENV_VARS;
use saispace_core::models::{
    CreateGoalInput, CreateProjectInput, GoalStatus, MemoryKind, SaveMemoryInput, TaskStatus,
    TimelineDay, TimelineItem, TimelineKind, UpdateGoalInput, UpdateTaskInput,
};
use saispace_core::{CoreError, Db, short_id};
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

fn git(dir: &Path, date: &str, args: &[&str]) {
    let mut command = Command::new("git");
    for var in REPO_ENV_VARS {
        command.env_remove(var);
    }
    let status = command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .status()
        .expect("git should run");
    assert!(status.success(), "git {args:?} failed");
}

fn commit_file(dir: &Path, date: &str, file: &str, message: &[&str]) {
    std::fs::write(dir.join(file), format!("{file} {date}\n")).expect("write file");
    git(dir, date, &["add", file]);
    let mut args = vec!["commit", "--quiet"];
    for paragraph in message {
        args.extend(["-m", paragraph]);
    }
    git(dir, date, &args);
}

async fn setup_db() -> Db {
    let db = Db::connect_in_memory().await.expect("in-memory db");
    db.migrate().await.expect("migrations");
    db
}

fn day<'a>(days: &'a [TimelineDay], date: &str) -> &'a TimelineDay {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("date");
    days.iter()
        .find(|d| d.date == date)
        .unwrap_or_else(|| panic!("missing day {date}"))
}

fn titled<'a>(day: &'a TimelineDay, title: &str) -> &'a TimelineItem {
    day.items
        .iter()
        .find(|item| item.title == title)
        .unwrap_or_else(|| panic!("missing item {title} in {:?}", day.items))
}

/// Main line: a direct commit, a merged PR (with a hidden branch commit), a
/// squashed PR, a sync merge, a commit naming a task by short id and an
/// annotated release tag.
fn build_repo(dir: &Path, task_ref: &str) {
    let d1 = "2026-11-01T10:00:00Z";
    let d2 = "2026-11-02T10:00:00Z";
    let d3 = "2026-11-03T12:00:00Z";
    git(dir, d1, &["init", "--quiet", "-b", "main"]);
    git(
        dir,
        d1,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:acme/facturas.git",
        ],
    );
    commit_file(dir, d1, "README.md", &["chore: initial commit"]);

    git(dir, d2, &["checkout", "--quiet", "-b", "export"]);
    commit_file(dir, d2, "export.rs", &["wip export"]);
    git(dir, d2, &["checkout", "--quiet", "main"]);
    git(
        dir,
        d2,
        &[
            "merge",
            "--quiet",
            "--no-ff",
            "export",
            "-m",
            "Merge pull request #2 from acme/export",
            "-m",
            "feat: exportar facturas en PDF",
        ],
    );

    commit_file(
        dir,
        d3,
        "totals.rs",
        &["fix(billing): round invoice totals (#3)"],
    );
    git(dir, d3, &["checkout", "--quiet", "-b", "sync"]);
    commit_file(dir, d3, "sync.txt", &["sync work"]);
    git(dir, d3, &["checkout", "--quiet", "main"]);
    git(
        dir,
        d3,
        &[
            "merge",
            "--quiet",
            "--no-ff",
            "sync",
            "-m",
            "Merge branch 'sync' into main",
        ],
    );
    commit_file(
        dir,
        d3,
        "tidy.rs",
        &["refactor: tidy modules", &format!("Task {task_ref}")],
    );
    git(dir, d3, &["tag", "-a", "v0.1.0", "-m", "Primera beta"]);
}

/// A project at `dir` with a goal of one task and an unrelated goal.
async fn seed_plan(db: &Db, dir: &Path) -> (Uuid, Uuid, Uuid) {
    let project = db
        .create_project(CreateProjectInput {
            name: "facturas".to_string(),
            path: dir.display().to_string(),
        })
        .await
        .expect("create project");

    let created = db
        .create_goal_with_tasks(
            project.id,
            "Exportar facturas en PDF".to_string(),
            Some("Cada factura se descarga en PDF".to_string()),
            &["Generar el PDF de cada factura".to_string()],
        )
        .await
        .expect("create goal");
    db.create_goal(CreateGoalInput {
        project_id: project.id,
        title: "Cobros recurrentes".to_string(),
        criteria: None,
        tasks: vec![],
    })
    .await
    .expect("second goal");

    (project.id, created.goal.id, created.tasks[0].id)
}

/// Completes the task and its goal, and records a decision and a note.
async fn finish_plan(db: &Db, project_id: Uuid, goal_id: Uuid, task_id: Uuid) {
    db.update_task(
        task_id,
        UpdateTaskInput {
            status: Some(TaskStatus::Done),
            ..UpdateTaskInput::default()
        },
    )
    .await
    .expect("complete task");
    db.update_goal(
        goal_id,
        UpdateGoalInput {
            status: Some(GoalStatus::Completed),
            ..UpdateGoalInput::default()
        },
    )
    .await
    .expect("complete goal");
    for (kind, title) in [
        (MemoryKind::Decision, "Generar los PDF con Typst"),
        (MemoryKind::Note, "Nota suelta"),
    ] {
        db.save_memory(SaveMemoryInput {
            project_id: Some(project_id),
            project_name: None,
            kind,
            title: title.to_string(),
            content: "What: x".to_string(),
            topic_key: None,
        })
        .await
        .expect("save memory");
    }
}

/// Builds a repo whose main line mixes a merged PR, a squashed PR, a sync
/// merge, a direct commit and an annotated release tag, plus a plan with a
/// finished goal, and checks the timeline reads like a changelog.
#[tokio::test]
async fn timeline_mixes_plan_and_first_parent_git_history() {
    let repo = tempfile::tempdir().expect("tempdir");
    let dir = repo.path();
    let db = setup_db().await;
    let (project_id, goal_id, task_id) = seed_plan(&db, dir).await;
    build_repo(dir, &short_id(task_id));
    finish_plan(&db, project_id, goal_id, task_id).await;

    let days = db
        .project_timeline(project_id, &Utc)
        .await
        .expect("timeline");

    let dates: Vec<String> = days.iter().map(|d| d.date.to_string()).collect();
    let today = Utc::now().date_naive().to_string();
    assert_eq!(
        dates,
        vec![today.as_str(), "2026-11-03", "2026-11-02", "2026-11-01"]
    );

    let plan_day = &days[0];
    let goal = titled(plan_day, "Exportar facturas en PDF");
    assert_eq!(goal.kind, TimelineKind::GoalCompleted);
    assert_eq!(
        goal.detail.as_deref(),
        Some("Cada factura se descarga en PDF")
    );
    let done = titled(plan_day, "Generar el PDF de cada factura");
    assert_eq!(done.kind, TimelineKind::TaskCompleted);
    assert_eq!(done.goal.as_deref(), Some("Exportar facturas en PDF"));
    let decision = titled(plan_day, "Generar los PDF con Typst");
    assert_eq!(decision.kind, TimelineKind::Memory);
    assert_eq!(decision.label.as_deref(), Some("Decisión"));
    assert!(plan_day.items.iter().all(|i| i.title != "Nota suelta"));

    let release_day = day(&days, "2026-11-03");
    let titles: Vec<&str> = release_day.items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles.len(), 3, "sync merge must be skipped: {titles:?}");
    let release = titled(release_day, "v0.1.0");
    assert_eq!(release.kind, TimelineKind::Release);
    assert_eq!(release.detail.as_deref(), Some("Primera beta"));
    assert_eq!(
        release.link.as_deref(),
        Some("https://github.com/acme/facturas/releases/tag/v0.1.0")
    );
    let fix = titled(release_day, "Round invoice totals");
    assert_eq!(fix.label.as_deref(), Some("Arreglo"));
    assert_eq!(fix.pr, Some(3));
    assert_eq!(
        fix.link.as_deref(),
        Some("https://github.com/acme/facturas/pull/3")
    );
    assert_eq!(fix.files, vec!["totals.rs"]);
    assert_eq!(fix.goal, None);
    let tidy = titled(release_day, "Tidy modules");
    assert_eq!(tidy.label.as_deref(), Some("Mejora interna"));
    assert_eq!(
        tidy.goal.as_deref(),
        Some("Exportar facturas en PDF"),
        "an explicit task id ties the commit to its goal"
    );
    let tidy_hash = tidy.hash.as_deref().expect("hash");
    assert_eq!(tidy_hash.len(), 7);
    assert!(tidy.link.as_deref().is_some_and(|l| l.starts_with(&format!(
        "https://github.com/acme/facturas/commit/{tidy_hash}"
    ))));

    let merge_day = day(&days, "2026-11-02");
    assert_eq!(merge_day.items.len(), 1, "branch commits stay hidden");
    let merged = &merge_day.items[0];
    assert_eq!(merged.title, "Exportar facturas en PDF");
    assert_eq!(merged.label.as_deref(), Some("Nuevo"));
    assert_eq!(merged.pr, Some(2));
    assert_eq!(merged.files, vec!["export.rs"]);
    assert_eq!(
        merged.goal.as_deref(),
        Some("Exportar facturas en PDF"),
        "shared words tie the PR to the goal"
    );

    let first = day(&days, "2026-11-01");
    assert_eq!(first.items[0].label.as_deref(), Some("Mantenimiento"));
    assert_eq!(first.items[0].title, "Initial commit");
}

#[tokio::test]
async fn timeline_without_git_shows_plan_and_unknown_project_fails() {
    let folder = tempfile::tempdir().expect("tempdir");
    let db = setup_db().await;
    let project = db
        .create_project(CreateProjectInput {
            name: "sin-git".to_string(),
            path: folder.path().display().to_string(),
        })
        .await
        .expect("create project");
    let created = db
        .create_goal_with_tasks(project.id, "Lanzar".to_string(), None, &[])
        .await
        .expect("goal");
    db.update_goal(
        created.goal.id,
        UpdateGoalInput {
            status: Some(GoalStatus::Completed),
            ..UpdateGoalInput::default()
        },
    )
    .await
    .expect("complete goal");

    let days = db
        .project_timeline(project.id, &Utc)
        .await
        .expect("timeline");
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].items.len(), 1);
    assert_eq!(days[0].items[0].kind, TimelineKind::GoalCompleted);

    let missing = db.project_timeline(Uuid::now_v7(), &Utc).await;
    assert!(matches!(missing, Err(CoreError::NotFound { .. })));
}
