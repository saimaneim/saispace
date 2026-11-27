use saispace_core::learning::{CORRECTION_REMINDER, STOP_REMINDER};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct NudgeSandbox {
    root: PathBuf,
}

impl NudgeSandbox {
    fn new(tag: &str) -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("sai-nudge-{tag}-{n}"));
        fs::create_dir_all(&root).expect("mkdir sandbox");
        Self { root }
    }

    fn db_path(&self) -> PathBuf {
        self.root.join("data/saispace.db")
    }

    fn run_sai(&self, args: &[&str], stdin: Option<&str>, extra_env: &[(&str, &Path)]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_sai"));
        cmd.args(args)
            .current_dir(&self.root)
            .env("SAISPACE_DB_PATH", self.db_path())
            .env("HOME", &self.root)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
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

impl Drop for NudgeSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn nudge_user_prompt_submit_and_opencode_shape() {
    let sb = NudgeSandbox::new("prompt");

    let correction = sb.run_sai(
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","prompt":"No hagas eso, usa bun"}"#),
        &[],
    );
    assert!(correction.status.success());
    assert_eq!(stdout_str(&correction).trim(), CORRECTION_REMINDER);
    assert!(!sb.db_path().exists(), "nudge must never create the DB");

    let neutral = sb.run_sai(
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","prompt":"run it again"}"#),
        &[],
    );
    assert!(neutral.status.success());
    assert_eq!(stdout_str(&neutral), "");

    let malformed = sb.run_sai(&["memory", "nudge"], Some("{{{not json"), &[]);
    assert!(malformed.status.success());
    assert_eq!(stdout_str(&malformed), "");
    assert_eq!(malformed.stderr, b"");
}

#[test]
fn nudge_stop_blocks_once_per_session_and_respects_saves_and_stop_hook_active() {
    let sb = NudgeSandbox::new("stop");
    let transcript = sb.root.join("session-1.jsonl");
    let unsaved = [
        r#"{"type":"user","message":{"content":"Te dije que uses pnpm"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{}}]}}"#,
    ]
    .join("\n");
    fs::write(&transcript, unsaved).expect("write transcript");

    let active_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "sess-1",
        "transcript_path": transcript,
        "stop_hook_active": true,
    })
    .to_string();
    let skipped_active = sb.run_sai(&["memory", "nudge"], Some(&active_payload), &[]);
    assert!(skipped_active.status.success());
    assert_eq!(stdout_str(&skipped_active), "");

    let stop_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "sess-1",
        "transcript_path": transcript,
        "stop_hook_active": false,
    })
    .to_string();
    let first_stop = sb.run_sai(&["memory", "nudge"], Some(&stop_payload), &[]);
    assert!(first_stop.status.success());
    let decision: serde_json::Value =
        serde_json::from_slice(&first_stop.stdout).expect("block json");
    assert_eq!(decision["decision"], "block");
    assert_eq!(decision["reason"], STOP_REMINDER);
    assert!(sb.root.join("data/nudged/sess-1").is_file());
    assert!(!sb.db_path().exists(), "stop nudge must not touch DB");

    let second_stop = sb.run_sai(&["memory", "nudge"], Some(&stop_payload), &[]);
    assert!(second_stop.status.success());
    assert_eq!(stdout_str(&second_stop), "");

    let failing_tools = [
        r#"{"type":"user","message":{"content":"revisa esto"}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}"#,
    ]
    .join("\n");
    let transcript_2 = sb.root.join("session-2.jsonl");
    fs::write(&transcript_2, failing_tools).expect("write transcript 2");
    let fail_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "sess-2",
        "transcript_path": transcript_2,
        "stop_hook_active": false,
    })
    .to_string();
    let fail_stop = sb.run_sai(&["memory", "nudge"], Some(&fail_payload), &[]);
    assert!(fail_stop.status.success());
    let fail_decision: serde_json::Value =
        serde_json::from_slice(&fail_stop.stdout).expect("fail block json");
    assert_eq!(fail_decision["decision"], "block");

    let saved = [
        r#"{"type":"user","message":{"content":"No hagas eso"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__saispace__save_memory","input":{}}]}}"#,
    ]
    .join("\n");
    let transcript_3 = sb.root.join("session-3.jsonl");
    fs::write(&transcript_3, saved).expect("write transcript 3");
    let saved_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "sess-3",
        "transcript_path": transcript_3,
    })
    .to_string();
    let saved_stop = sb.run_sai(&["memory", "nudge"], Some(&saved_payload), &[]);
    assert!(saved_stop.status.success());
    assert_eq!(stdout_str(&saved_stop), "");

    let traversal_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "../escape",
        "transcript_path": transcript,
        "stop_hook_active": false,
    })
    .to_string();
    let traversal = sb.run_sai(&["memory", "nudge"], Some(&traversal_payload), &[]);
    assert!(traversal.status.success());
    assert_eq!(stdout_str(&traversal), "");
    assert!(!sb.root.join("escape").exists());
}

#[allow(clippy::too_many_lines)] // one end-to-end scenario reads best in one piece
#[test]
fn nudge_logs_kpi_events_and_rotates_over_5mb() {
    let sb = NudgeSandbox::new("kpi-log");
    let log_path = sb.root.join("data/kpi-events.jsonl");
    let rotated_path = sb.root.join("data/kpi-events.jsonl.1");

    let correction = sb.run_sai(
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","session_id":"s-kpi","prompt":"No hagas eso"}"#),
        &[],
    );
    assert!(correction.status.success());

    let neutral = sb.run_sai(
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","session_id":"s-kpi","prompt":"ok sigue"}"#),
        &[],
    );
    assert!(neutral.status.success());

    let transcript = sb.root.join("s-kpi.jsonl");
    fs::write(
        &transcript,
        r#"{"type":"user","message":{"content":"No hagas eso"}}"#,
    )
    .expect("write transcript");
    let stop_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "s-kpi",
        "transcript_path": transcript,
        "stop_hook_active": false,
    })
    .to_string();
    assert!(
        sb.run_sai(&["memory", "nudge"], Some(&stop_payload), &[])
            .status
            .success()
    );

    // Second stop with stop_hook_active=true still logs an event with blocked=false.
    let second_payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "s-kpi",
        "transcript_path": transcript,
        "stop_hook_active": true,
    })
    .to_string();
    assert!(
        sb.run_sai(&["memory", "nudge"], Some(&second_payload), &[])
            .status
            .success()
    );

    let raw = fs::read_to_string(&log_path).expect("read kpi log");
    let lines: Vec<serde_json::Value> = raw
        .lines()
        .map(|l| serde_json::from_str(l).expect("valid json line"))
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["event"], "correction");
    assert_eq!(lines[0]["session_id"], "s-kpi");
    assert_eq!(lines[1]["event"], "stop");
    assert_eq!(lines[1]["corrections"], 1);
    assert_eq!(lines[1]["saved"], false);
    assert_eq!(lines[1]["blocked"], true);
    assert_eq!(lines[2]["event"], "stop");
    assert_eq!(lines[2]["blocked"], false);
    assert!(!sb.db_path().exists(), "nudge must never create DB");

    // Exceed 5 MB and trigger rotation.
    fs::write(&log_path, vec![b'x'; 5 * 1024 * 1024 + 10]).expect("write big log");
    let rotate_trigger = sb.run_sai(
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","prompt":"No uses eso"}"#),
        &[],
    );
    assert!(rotate_trigger.status.success());
    assert!(!rotated_path.exists(), "must never use legacy .jsonl.1");

    let list_rotated = || -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(sb.root.join("data"))
            .expect("read data dir")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                let is_jsonl = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"));
                is_jsonl
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n != "kpi-events.jsonl" && n.starts_with("kpi-events."))
            })
            .collect();
        files.sort();
        files
    };

    let first_rotated = list_rotated();
    assert_eq!(first_rotated.len(), 1);
    let new_raw = fs::read_to_string(&log_path).expect("read rotated fresh log");
    assert_eq!(new_raw.lines().count(), 1);

    // Second rotation creates a second unique rotated file without overwriting the first.
    fs::write(&log_path, vec![b'y'; 5 * 1024 * 1024 + 10]).expect("write second big log");
    let rotate_trigger_2 = sb.run_sai(
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","prompt":"Para de usar eso"}"#),
        &[],
    );
    assert!(rotate_trigger_2.status.success());
    let second_rotated = list_rotated();
    assert_eq!(second_rotated.len(), 2);

    // Rotated files older than 35 days are pruned; recent ones and kpi-events.jsonl stay.
    let expired_by_name = sb.root.join("data/kpi-events.1000.99.jsonl");
    fs::write(&expired_by_name, "{\"event\":\"correction\"}\n").expect("write expired");
    let recent_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("epoch")
        .as_millis()
        .saturating_sub(5 * 86_400 * 1_000);
    let recent_rotated = sb
        .root
        .join(format!("data/kpi-events.{recent_ms}.77.jsonl"));
    fs::write(&recent_rotated, "{\"event\":\"correction\"}\n").expect("write recent");

    let prune_trigger = sb.run_sai(
        &["memory", "nudge"],
        Some(r#"{"hook_event_name":"UserPromptSubmit","prompt":"No hagas eso tampoco"}"#),
        &[],
    );
    assert!(prune_trigger.status.success());
    assert!(
        !expired_by_name.exists(),
        "expired rotated log must be pruned"
    );
    assert!(recent_rotated.is_file(), "recent rotated log must stay");
    assert!(log_path.is_file(), "active kpi-events.jsonl must stay");
}

#[test]
fn setup_claude_respects_claude_config_dir_and_uninstalls() {
    let sb = NudgeSandbox::new("setup-claude");
    let custom_dir = sb.root.join("custom-claude");

    let install = sb.run_sai(
        &["setup", "claude"],
        None,
        &[("CLAUDE_CONFIG_DIR", &custom_dir)],
    );
    assert!(install.status.success());
    assert!(custom_dir.join("settings.json").is_file());

    let uninstall = sb.run_sai(
        &["setup", "claude", "--uninstall"],
        None,
        &[("CLAUDE_CONFIG_DIR", &custom_dir)],
    );
    assert!(uninstall.status.success());
    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(custom_dir.join("settings.json")).expect("read settings"),
    )
    .expect("json");
    assert!(settings.get("hooks").is_none());
}

#[test]
fn stop_names_repeated_errors_and_suggests_a_trigger() {
    let sb = NudgeSandbox::new("repeat");
    let fail = r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"Exit code 144"}]}}"#;
    let transcript = sb.root.join("t.jsonl");
    fs::write(&transcript, format!("{fail}\n{fail}\n")).expect("write transcript");
    let payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "repeat-1",
        "transcript_path": transcript.display().to_string(),
    })
    .to_string();
    let out = sb.run_sai(&["memory", "nudge"], Some(&payload), &[]);
    assert!(out.status.success());
    let value: serde_json::Value =
        serde_json::from_str(stdout_str(&out).trim()).expect("block json");
    assert_eq!(value["decision"], "block");
    let reason = value["reason"].as_str().expect("reason");
    assert!(reason.starts_with(STOP_REMINDER), "{reason}");
    assert!(reason.contains("`exit code 144` (2 veces)"), "{reason}");
    assert!(reason.contains("--trigger"), "{reason}");
}
