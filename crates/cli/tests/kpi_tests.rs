use chrono::{Duration, Utc};
use saispace_core::Db;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct KpiSandbox {
    root: PathBuf,
}

impl KpiSandbox {
    fn new(tag: &str) -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("sai-kpi-{tag}-{n}"));
        fs::create_dir_all(root.join("dev/repo/.git")).expect("mkdir repo");
        Self { root }
    }

    fn repo(&self) -> PathBuf {
        self.root.join("dev/repo")
    }

    fn db_path(&self) -> PathBuf {
        self.root.join("db/saispace.db")
    }

    fn sai(&self, cwd: &Path, args: &[&str], stdin: Option<&str>) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_sai"));
        cmd.args(args)
            .current_dir(cwd)
            .env("SAISPACE_DB_PATH", self.db_path())
            .env("HOME", &self.root)
            .env("SAISPACE_AGENT_NAME", "claude")
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn sai");
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(input.as_bytes())
                .expect("write stdin");
        }
        child.wait_with_output().expect("wait sai")
    }
}

impl Drop for KpiSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn wait_for_snapshot(db: &Db, day: &str, scope: &str) {
    for _ in 0..200 {
        if matches!(db.has_snapshot_for_day(day, scope).await, Ok(true)) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for background snapshot day={day} scope={scope}");
}

async fn seed_historical_snapshots(db_path: &Path, repo: &Path) -> String {
    let db = Db::open(db_path).await.expect("open db");
    let now = Utc::now();
    let today_str = now.date_naive().format("%Y-%m-%d").to_string();
    let repo_canon = repo.canonicalize().expect("canonicalize repo");
    let project = db
        .find_project_for_path(&repo_canon)
        .await
        .expect("find project")
        .expect("repo registered");
    let project_scope = project.id.to_string();

    wait_for_snapshot(&db, &today_str, &project_scope).await;
    wait_for_snapshot(&db, &today_str, "global").await;

    db.save_memory_at(
        saispace_core::models::SaveMemoryInput {
            project_id: None,
            project_name: None,
            kind: saispace_core::models::MemoryKind::Note,
            title: "Database connection pooling configuration".into(),
            content: "Pool 1".into(),
            topic_key: None,
        },
        now - Duration::days(30),
        Some(now - Duration::days(30)),
    )
    .await
    .expect("save extra");
    db.take_daily_snapshot(now - Duration::days(30), None)
        .await
        .expect("snap -30d");

    let dup = db
        .save_memory_at(
            saispace_core::models::SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: saispace_core::models::MemoryKind::Note,
                title: "Database connection pooling settings".into(),
                content: "Pool 2".into(),
                topic_key: None,
            },
            now - Duration::days(7),
            Some(now - Duration::days(7)),
        )
        .await
        .expect("save dup");
    db.take_daily_snapshot(now - Duration::days(7), None)
        .await
        .expect("snap -7d");

    db.delete_memory(dup.id).await.expect("delete dup");
    project_scope
}

#[allow(clippy::too_many_lines)] // one end-to-end scenario reads best in one piece
#[tokio::test]
async fn context_auto_snapshots_and_kpi_reports_deltas() {
    let sb = KpiSandbox::new("e2e");
    let repo = sb.repo();

    let save = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Regla de proyecto",
            "--content",
            "Usar biome siempre",
            "--kind",
            "decision",
            "--topic",
            "biome",
        ],
        None,
    );
    assert!(
        save.status.success(),
        "{}",
        String::from_utf8_lossy(&save.stderr)
    );

    let nudge = sb.sai(
        &repo,
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","session_id":"s1","prompt":"No hagas eso"}"#),
    );
    assert!(nudge.status.success());

    let transcript = sb.root.join("t.jsonl");
    fs::write(
        &transcript,
        r#"{"type":"user","message":{"content":"No hagas eso"}}"#,
    )
    .expect("write transcript");
    let stop_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "s1",
        "transcript_path": transcript,
    })
    .to_string();
    assert!(
        sb.sai(&repo, &["memory", "nudge"], Some(&stop_payload))
            .status
            .success()
    );

    let ctx = sb.sai(&repo, &["memory", "context"], None);
    assert!(
        ctx.status.success(),
        "{}",
        String::from_utf8_lossy(&ctx.stderr)
    );
    assert!(String::from_utf8_lossy(&ctx.stdout).contains("Regla de proyecto"));

    let project_scope = seed_historical_snapshots(&sb.db_path(), &repo).await;

    let proj_out = sb.sai(&repo, &["kpi", "--json"], None);
    assert!(proj_out.status.success());
    let proj_json: serde_json::Value = serde_json::from_slice(&proj_out.stdout).expect("proj json");
    assert_eq!(proj_json["scope"], project_scope);
    assert_eq!(proj_json["project_name"], "repo");

    let by_name_out = sb.sai(&repo, &["kpi", "--project", "repo", "--json"], None);
    assert!(by_name_out.status.success());
    let by_name_json: serde_json::Value =
        serde_json::from_slice(&by_name_out.stdout).expect("by_name json");
    assert_eq!(by_name_json["scope"], project_scope);
    assert_eq!(by_name_json["project_name"], "repo");

    let by_name_text = sb.sai(&repo, &["kpi", "--project", "repo"], None);
    assert!(by_name_text.status.success());
    assert!(String::from_utf8_lossy(&by_name_text.stdout).contains("proyecto repo"));

    let global_json_out = sb.sai(&repo, &["kpi", "--global", "--json"], None);
    assert!(global_json_out.status.success());
    let global_json: serde_json::Value =
        serde_json::from_slice(&global_json_out.stdout).expect("global json");
    assert_eq!(global_json["scope"], "global");
    let metrics = global_json["metrics"].as_array().expect("metrics array");

    let active_row = metrics
        .iter()
        .find(|m| m["name"] == "memories_active")
        .expect("memories_active");
    assert_eq!(active_row["value"], 2.0);
    assert_eq!(active_row["delta_7d"], -1.0);
    assert_eq!(active_row["trend_7d"], "moved");
    assert_eq!(active_row["delta_30d"], 0.0);
    assert_eq!(active_row["trend_30d"], "same");

    let dupes_row = metrics
        .iter()
        .find(|m| m["name"] == "duplicate_pairs")
        .expect("duplicate_pairs");
    assert_eq!(dupes_row["value"], 0.0);
    assert_eq!(dupes_row["delta_7d"], -1.0);
    assert_eq!(dupes_row["trend_7d"], "better");

    let conv_row = metrics
        .iter()
        .find(|m| m["name"] == "lesson_conversion_7d")
        .expect("lesson_conversion_7d");
    assert_eq!(conv_row["value"], 0.0);

    let text_out = sb.sai(&repo, &["kpi", "--global"], None);
    assert!(text_out.status.success());
    let text = String::from_utf8_lossy(&text_out.stdout);
    assert!(text.contains("Memorias activas"));
    assert!(text.contains("✓"));
    assert!(text.contains("·"), "volumes move without a verdict");

    let missing_proj = sb.sai(&repo, &["kpi", "--project", "no-existe"], None);
    assert_eq!(missing_proj.status.code(), Some(2));
}

#[tokio::test]
async fn kpi_project_named_global_and_duplicate_project_names_use_ids() {
    let sb = KpiSandbox::new("proj-ids");
    let db = Db::open(&sb.db_path()).await.expect("open db");
    let p_global = db
        .create_project(saispace_core::models::CreateProjectInput {
            name: "global".into(),
            path: sb.root.join("p-global").to_string_lossy().into_owned(),
        })
        .await
        .expect("p_global");
    let p_dup1 = db
        .create_project(saispace_core::models::CreateProjectInput {
            name: "dup".into(),
            path: sb.root.join("dup1").to_string_lossy().into_owned(),
        })
        .await
        .expect("dup1");
    let _p_dup2 = db
        .create_project(saispace_core::models::CreateProjectInput {
            name: "dup".into(),
            path: sb.root.join("dup2").to_string_lossy().into_owned(),
        })
        .await
        .expect("dup2");

    let snap = sb.sai(&sb.repo(), &["kpi", "snapshot"], None);
    assert!(snap.status.success());

    let proj_global = sb.sai(&sb.repo(), &["kpi", "--project", "global", "--json"], None);
    assert!(proj_global.status.success());
    let pg_json: serde_json::Value = serde_json::from_slice(&proj_global.stdout).expect("pg json");
    assert_eq!(pg_json["scope"], p_global.id.to_string());
    assert_eq!(pg_json["project_name"], "global");

    let real_global = sb.sai(&sb.repo(), &["kpi", "--global", "--json"], None);
    assert!(real_global.status.success());
    let rg_json: serde_json::Value = serde_json::from_slice(&real_global.stdout).expect("rg json");
    assert_eq!(rg_json["scope"], "global");
    assert!(rg_json.get("project_name").is_none());

    let ambiguous = sb.sai(&sb.repo(), &["kpi", "--project", "dup"], None);
    assert_eq!(ambiguous.status.code(), Some(2));

    let disambiguated = sb.sai(
        &sb.repo(),
        &["kpi", "--project", &p_dup1.id.to_string(), "--json"],
        None,
    );
    assert!(disambiguated.status.success());
    let d_json: serde_json::Value =
        serde_json::from_slice(&disambiguated.stdout).expect("disambiguated json");
    assert_eq!(d_json["scope"], p_dup1.id.to_string());
    assert_eq!(d_json["project_name"], "dup");
}
