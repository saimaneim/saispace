use saispace_core::models::{
    CreateProjectInput, CreateTaskInput, SearchKind, UpdateProjectInput, UpdateTaskInput,
    UpsertTechRuleInput,
};
use saispace_core::{CoreError, Db};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("saispace-it-{}", Uuid::now_v7()));
        fs::create_dir_all(&dir).expect("create temp project");
        Self(dir)
    }

    fn write(&self, relative: &str, content: &str) {
        let target = self.0.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("create fixture dirs");
        }
        fs::write(target, content).expect("write fixture");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn setup() -> Db {
    let db = Db::connect_in_memory().await.expect("in-memory db");
    db.migrate().await.expect("migrations");
    db
}

async fn project(db: &Db, name: &str, path: &str, notes: Option<&str>) -> Uuid {
    db.create_project(CreateProjectInput {
        name: name.to_string(),
        path: path.to_string(),
        context_notes: notes.map(str::to_string),
    })
    .await
    .expect("create project")
    .id
}

async fn count(db: &Db, query: &str, project_id: Option<Uuid>) -> usize {
    db.search(query, project_id, None)
        .await
        .expect("search")
        .len()
}

fn rule(technology: &str, version: Option<&str>, text: &str) -> UpsertTechRuleInput {
    UpsertTechRuleInput {
        technology: technology.to_string(),
        version: version.map(str::to_string),
        rule: text.to_string(),
    }
}

#[tokio::test]
async fn sync_detects_cargo_and_npm_versions_from_real_manifests() {
    let db = setup().await;
    let fixture = TempProject::new();
    fixture.write(
        "Cargo.toml",
        "[dependencies]\ntauri = \"2\"\nserde = { version = \"1.0\", features = [\"derive\"] }\nlocal = { path = \"../local\" }\n",
    );
    fixture.write(
        "Cargo.lock",
        "[[package]]\nname = \"tauri\"\nversion = \"2.12.7\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.229\"\n",
    );
    fixture.write(
        "web/package.json",
        r#"{"dependencies":{"react":"^19.0.0"},"devDependencies":{"vite":"~8.1.0"}}"#,
    );
    fixture.write(
        "web/node_modules/react/package.json",
        r#"{"version":"19.3.1"}"#,
    );
    fixture.write(
        "node_modules/ignored/package.json",
        r#"{"dependencies":{"leak":"1.0.0"}}"#,
    );

    let id = project(&db, "demo", &fixture.path().to_string_lossy(), None).await;
    let stack = db.sync_project_stack(id).await.expect("sync");

    let version_of = |name: &str| {
        stack
            .iter()
            .find(|t| t.name == name)
            .map(|t| (t.version.as_str(), t.ecosystem.as_str()))
    };
    assert_eq!(version_of("tauri"), Some(("2.12.7", "cargo")));
    assert_eq!(version_of("serde"), Some(("1.0.229", "cargo")));
    assert_eq!(version_of("react"), Some(("19.3.1", "npm")));
    assert_eq!(version_of("vite"), Some(("8.1.0", "npm")));
    assert_eq!(version_of("local"), None);
    assert_eq!(version_of("leak"), None);

    fixture.write("Cargo.toml", "[dependencies]\ntauri = \"2\"\n");
    let resynced = db.sync_project_stack(id).await.expect("resync");
    assert!(resynced.iter().all(|t| t.name != "serde"));
    assert_eq!(
        db.list_technologies(id).await.expect("list").len(),
        resynced.len()
    );
}

#[tokio::test]
async fn sync_rejects_missing_directory_and_unknown_project() {
    let db = setup().await;
    let id = project(&db, "ghost", "/definitely/not/here", None).await;
    assert!(matches!(
        db.sync_project_stack(id).await,
        Err(CoreError::Validation(_))
    ));
    assert!(matches!(
        db.sync_project_stack(Uuid::now_v7()).await,
        Err(CoreError::NotFound {
            entity: "project",
            ..
        })
    ));
}

#[tokio::test]
async fn tech_rules_upsert_validate_and_delete() {
    let db = setup().await;

    let first = db
        .upsert_tech_rule(rule(
            " Tauri ",
            Some("2"),
            "Use capabilities, not allowlist.",
        ))
        .await
        .expect("insert");
    assert_eq!(first.technology, "tauri");
    assert_eq!(first.version_req.as_deref(), Some("2"));

    let replaced = db
        .upsert_tech_rule(rule(
            "tauri",
            Some("2"),
            "Permissions live in capabilities/*.json.",
        ))
        .await
        .expect("replace");
    assert_eq!(replaced.id, first.id);
    assert_eq!(replaced.rule, "Permissions live in capabilities/*.json.");

    let unversioned = db
        .upsert_tech_rule(rule("tauri", None, "Prefer typed commands."))
        .await
        .expect("unversioned");
    assert_ne!(unversioned.id, first.id);
    assert_eq!(unversioned.version_req, None);

    db.upsert_tech_rule(rule("react", Some("  "), "Prefer function components."))
        .await
        .expect("blank version means none");

    assert_eq!(db.list_tech_rules(None).await.expect("all").len(), 3);
    assert_eq!(
        db.list_tech_rules(Some("TAURI"))
            .await
            .expect("filtered")
            .len(),
        2
    );

    for bad in [
        rule("", None, "x"),
        rule("tauri", None, "   "),
        rule("tauri", None, &"x".repeat(501)),
        rule("tauri", Some(&"9".repeat(41)), "x"),
    ] {
        assert!(matches!(
            db.upsert_tech_rule(bad).await,
            Err(CoreError::Validation(_))
        ));
    }

    db.delete_tech_rule(first.id).await.expect("delete");
    assert!(matches!(
        db.delete_tech_rule(first.id).await,
        Err(CoreError::NotFound {
            entity: "tech_rule",
            ..
        })
    ));
}

#[tokio::test]
async fn rules_for_project_follow_the_detected_stack() {
    let db = setup().await;
    let fixture = TempProject::new();
    fixture.write(
        "Cargo.toml",
        "[dependencies]\ntauri = \"2\"\nsqlx = \"0.8\"\n",
    );
    fixture.write(
        "Cargo.lock",
        "[[package]]\nname = \"tauri\"\nversion = \"2.12.7\"\n",
    );
    let id = project(&db, "app", &fixture.path().to_string_lossy(), None).await;
    db.sync_project_stack(id).await.expect("sync");

    db.upsert_tech_rule(rule("tauri", Some("2"), "v2 rule"))
        .await
        .expect("r1");
    db.upsert_tech_rule(rule("tauri", Some("1"), "v1 rule"))
        .await
        .expect("r2");
    db.upsert_tech_rule(rule("sqlx", None, "any sqlx"))
        .await
        .expect("r3");
    db.upsert_tech_rule(rule("react", None, "not in stack"))
        .await
        .expect("r4");

    let mut matched: Vec<String> = db
        .tech_rules_for_project(id)
        .await
        .expect("matched")
        .into_iter()
        .map(|r| r.rule)
        .collect();
    matched.sort();
    assert_eq!(matched, ["any sqlx", "v2 rule"]);
}

async fn assert_rule_lifecycle(db: &Db, other: uuid::Uuid) {
    let saved = db
        .upsert_tech_rule(rule(
            "tauri",
            Some("2"),
            "Declare permissions in capabilities files",
        ))
        .await
        .expect("rule");
    let rule_hits = db
        .search("permissions", Some(other), None)
        .await
        .expect("global rule");
    assert_eq!(rule_hits.len(), 1);
    assert_eq!(rule_hits[0].kind, SearchKind::TechRule);
    assert_eq!(rule_hits[0].project_id, None);
    assert_eq!(rule_hits[0].id, saved.id);

    db.upsert_tech_rule(rule("tauri", Some("2"), "Rewritten guidance"))
        .await
        .expect("rewrite rule");
    assert_eq!(count(db, "permissions", None).await, 0);
    assert_eq!(
        db.search("guidance", None, None)
            .await
            .expect("new rule")
            .len(),
        1
    );

    db.delete_tech_rule(saved.id).await.expect("delete rule");
    assert_eq!(count(db, "guidance", None).await, 0);
}

#[tokio::test]
async fn fts_tracks_project_notes_tasks_and_rules() {
    let db = setup().await;
    let id = project(
        &db,
        "alpha",
        "/p/alpha",
        Some("Usamos migración incremental con SQLite"),
    )
    .await;
    let other = project(
        &db,
        "beta",
        "/p/beta",
        Some("Notas sobre incremental en otro proyecto"),
    )
    .await;

    let hits = db
        .search("migracion", None, None)
        .await
        .expect("accent-insensitive");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, SearchKind::Project);
    assert_eq!(hits[0].id, id);
    assert!(hits[0].snippet.contains('['));

    assert_eq!(
        db.search("increm", None, None).await.expect("prefix").len(),
        2
    );
    let scoped = db.search("increm", Some(id), None).await.expect("scoped");
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].project_id, Some(id));

    db.update_project(
        id,
        UpdateProjectInput {
            context_notes: Some("ahora hablamos de tauri capabilities".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("update notes");
    assert_eq!(count(&db, "migracion", Some(id)).await, 0);
    assert_eq!(
        db.search("capabilities", None, None)
            .await
            .expect("new term")
            .len(),
        1
    );

    let task = db
        .create_task(CreateTaskInput {
            project_id: id,
            title: "Rediseñar el buscador".to_string(),
            priority: None,
        })
        .await
        .expect("task");
    let task_hits = db.search("buscador", None, None).await.expect("task hit");
    assert_eq!(task_hits.len(), 1);
    assert_eq!(task_hits[0].kind, SearchKind::Task);
    assert_eq!(task_hits[0].id, task.id);
    assert!(task_hits[0].snippet.contains('['));

    db.update_task(
        task.id,
        UpdateTaskInput {
            title: Some("Pulir indexado".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("rename task");
    assert_eq!(count(&db, "buscador", None).await, 0);
    assert_eq!(
        db.search("indexado", None, None)
            .await
            .expect("new title")
            .len(),
        1
    );

    assert_rule_lifecycle(&db, other).await;

    db.delete_project(id).await.expect("delete project");
    assert_eq!(count(&db, "capabilities", None).await, 0);
    assert_eq!(count(&db, "indexado", None).await, 0);
}

#[tokio::test]
async fn fts_survives_hostile_queries_and_limits_results() {
    let db = setup().await;
    let id = project(&db, "gamma", "/p/gamma", Some("alpha beta")).await;
    for n in 0..5 {
        db.create_task(CreateTaskInput {
            project_id: id,
            title: format!("alpha tarea {n}"),
            priority: None,
        })
        .await
        .expect("task");
    }

    for hostile in [
        "alpha\"",
        "alpha OR",
        "alpha*",
        "(alpha",
        "NEAR(alpha beta)",
        "col:alpha",
        "alpha -beta",
    ] {
        db.search(hostile, None, None)
            .await
            .unwrap_or_else(|e| panic!("query {hostile:?} failed: {e}"));
    }
    for empty in ["  ---  ", "\"", ""] {
        assert!(matches!(
            db.search(empty, None, None).await,
            Err(CoreError::Validation(_))
        ));
    }
    assert_eq!(
        db.search("alpha", None, Some(3))
            .await
            .expect("limited")
            .len(),
        3
    );
    assert_eq!(
        db.search("alpha", None, Some(0))
            .await
            .expect("clamped")
            .len(),
        1
    );
}
