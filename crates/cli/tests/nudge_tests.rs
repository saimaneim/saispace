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
