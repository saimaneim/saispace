use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// Host variables the test runner may inherit (it often runs inside Claude
/// Code or T3 Code): cleared so host detection is deterministic.
const HOST_VARS: &[&str] = &[
    "T3CODE_THREAD_ID",
    "T3CODE_CLI_PATH",
    "CLAUDECODE",
    "OPENCODE",
    "OPENCODE_BIN_PATH",
    "OPENCODE_SESSION_ID",
    "CODEX_HOME",
    "CODEX_SANDBOX",
    "GEMINI_CLI",
    "SAISPACE_HOST",
];

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("sai-recall-{tag}-{n}"));
        fs::create_dir_all(root.join("work/proj/.git")).expect("mkdir sandbox");
        Self { root }
    }

    fn project(&self) -> PathBuf {
        self.root.join("work/proj")
    }

    fn sai(&self, args: &[&str], stdin: Option<&str>, env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_sai"));
        cmd.args(args)
            .current_dir(self.project())
            .env("SAISPACE_DB_PATH", self.root.join("data/saispace.db"))
            .env("HOME", self.root.join("home"))
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for var in HOST_VARS {
            cmd.env_remove(var);
        }
        for (k, v) in env {
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

    fn ok(&self, args: &[&str], env: &[(&str, &str)]) -> String {
        let out = self.sai(args, None, env);
        assert!(
            out.status.success(),
            "sai {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn hook(&self, payload: &serde_json::Value) -> String {
        let out = self.sai(&["memory", "nudge"], Some(&payload.to_string()), &[]);
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn failure(session: &str, cwd: &str, command: &str, error: &str) -> serde_json::Value {
    serde_json::json!({
        "hook_event_name": "PostToolUseFailure",
        "session_id": session,
        "cwd": cwd,
        "tool_name": "Bash",
        "tool_input": { "command": command },
        "error": error,
    })
}

#[test]
#[allow(clippy::too_many_lines)]
fn recalls_a_lesson_once_per_session_when_its_error_repeats() {
    let sb = Sandbox::new("error");
    let cwd = sb.project().display().to_string();
    let saved = sb.ok(
        &[
            "memory",
            "save",
            "--global",
            "--kind",
            "bugfix",
            "--title",
            "pkill -f se mata a sí mismo",
            "--content",
            "What: exit 144.\nLearned: usa pkill -f \"[p]atrón\" o el PID.",
            "--trigger",
            "Exit code 144",
        ],
        &[],
    );
    assert!(saved.contains("disparadores: exit code 144"), "{saved}");

    let first = sb.hook(&failure("s1", &cwd, "pkill -f server", "Exit code 144"));
    let value: serde_json::Value = serde_json::from_str(first.trim()).expect("hook json");
    assert_eq!(
        value["hookSpecificOutput"]["hookEventName"],
        "PostToolUseFailure"
    );
    let context = value["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("context");
    assert!(context.contains("pkill -f se mata a sí mismo"), "{context}");
    assert!(
        context.contains("usa pkill -f \"[p]atrón\" o el PID."),
        "the Learned line is what comes back: {context}"
    );

    // Same session: already shown, stays quiet. A new session gets it again.
    assert_eq!(
        sb.hook(&failure("s1", &cwd, "pkill -f x", "Exit code 144"))
            .trim(),
        ""
    );
    assert!(
        sb.hook(&failure("s2", &cwd, "pkill -f x", "Exit code 144"))
            .contains("additionalContext")
    );

    // Unrelated error, saving the lesson itself, and file reads: no recall.
    assert_eq!(
        sb.hook(&failure("s3", &cwd, "ls", "Exit code 2\nls: cannot access"))
            .trim(),
        ""
    );
    assert_eq!(
        sb.hook(&failure(
            "s4",
            &cwd,
            "sai memory save --trigger 'Exit code 144'",
            "Exit code 144"
        ))
        .trim(),
        ""
    );
    let read = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "session_id": "s5",
        "cwd": cwd,
        "tool_name": "Read",
        "tool_input": { "file_path": "notes.md" },
        "tool_response": { "file": { "content": "we saw Exit code 144 yesterday" } },
    });
    assert_eq!(sb.hook(&read).trim(), "");

    // A failed edit of a file that quotes the error recalls nothing: only the
    // content tool's own error counts.
    let edit = serde_json::json!({
        "hook_event_name": "PostToolUseFailure",
        "session_id": "s5b",
        "cwd": cwd,
        "tool_name": "Edit",
        "tool_input": { "file_path": "notes.md", "new_string": "Exit code 144 al usar pkill" },
        "error": "File has been modified since read",
    });
    assert_eq!(sb.hook(&edit).trim(), "");

    // No session id: no once-per-session guard, so no recall.
    let mut anonymous = failure("s0", &cwd, "pkill -f x", "Exit code 144");
    anonymous
        .as_object_mut()
        .expect("object")
        .remove("session_id");
    assert_eq!(sb.hook(&anonymous).trim(), "");

    // A successful Bash call whose output carries the error still recalls.
    let ssh = sb.ok(
        &[
            "memory",
            "save",
            "--title",
            "Clave SSH con passphrase",
            "--content",
            "Learned: pide al usuario ejecutar ssh-add en su terminal.",
            "--trigger",
            "Permission denied (publickey)",
        ],
        &[],
    );
    assert!(ssh.contains("disparadores"), "{ssh}");
    let success = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "session_id": "s6",
        "cwd": cwd,
        "tool_name": "Bash",
        "tool_input": { "command": "ssh root@host ls" },
        "tool_response": { "stdout": "", "stderr": "root@1.2.3.4: Permission denied (publickey).\n" },
    });
    assert!(sb.hook(&success).contains("Clave SSH con passphrase"));

    // The project lesson does not leak into another project.
    let other = sb.root.join("work/other");
    fs::create_dir_all(other.join(".git")).expect("mkdir other");
    let elsewhere = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "session_id": "s7",
        "cwd": other.display().to_string(),
        "tool_name": "Bash",
        "tool_input": { "command": "ssh root@host ls" },
        "tool_response": { "stderr": "Permission denied (publickey)." },
    });
    assert_eq!(sb.hook(&elsewhere).trim(), "");

    let shown = sb.ok(
        &[
            "memory",
            "get",
            "--json",
            ssh.split_whitespace().next().unwrap_or_default(),
        ],
        &[],
    );
    let shown: serde_json::Value = serde_json::from_str(&shown).expect("get json");
    assert_eq!(shown["triggers"][0], "permission denied (publickey)");
    assert_eq!(
        shown["memory"]["use_count"], 1,
        "the recall counts as a use (get shows the count before its own)"
    );
}

#[test]
fn rejects_generic_triggers_without_saving() {
    let sb = Sandbox::new("generic");
    let out = sb.sai(
        &[
            "memory",
            "save",
            "--global",
            "--title",
            "x",
            "--content",
            "y",
            "--trigger",
            "error",
        ],
        None,
        &[],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("genérico"));
    let list = sb.ok(&["memory", "list", "--all"], &[]);
    assert!(!list.contains(" x "), "nothing saved: {list}");
}

#[test]
fn session_start_shows_host_lessons_only_inside_that_host() {
    let sb = Sandbox::new("env");
    sb.ok(
        &[
            "memory",
            "save",
            "--global",
            "--kind",
            "preference",
            "--title",
            "T3 Code no tiene TTY",
            "--content",
            "What: el `!` del chat no ejecuta.\nLearned: para passphrase o sudo, dale al usuario un comando para su terminal.",
            "--trigger",
            "env:t3code",
        ],
        &[],
    );
    let outside = sb.ok(&["memory", "context"], &[]);
    assert!(!outside.contains("## Entorno"), "{outside}");
    assert!(
        outside.contains("T3 Code no tiene TTY"),
        "still a preference: {outside}"
    );

    let inside = sb.ok(&["memory", "context"], &[("T3CODE_THREAD_ID", "thread-1")]);
    assert!(inside.contains("## Entorno: T3 Code"), "{inside}");
    assert!(
        inside.contains("dale al usuario un comando para su terminal"),
        "{inside}"
    );
    assert_eq!(
        inside.matches("T3 Code no tiene TTY").count(),
        1,
        "shown once, under Entorno: {inside}"
    );

    let via_plugin = sb.ok(&["memory", "context"], &[("SAISPACE_HOST", "opencode")]);
    assert!(!via_plugin.contains("## Entorno"), "{via_plugin}");

    let cleared = sb.ok(&["memory", "edit", "--no-triggers", &short(&outside)], &[]);
    assert!(cleared.contains("disparadores: ninguno"), "{cleared}");
    let after = sb.ok(&["memory", "context"], &[("T3CODE_THREAD_ID", "thread-1")]);
    assert!(!after.contains("## Entorno"), "{after}");
}

/// Short id of the first preference line (`… [abcd1234]`).
fn short(index: &str) -> String {
    index
        .lines()
        .find(|l| l.contains("T3 Code no tiene TTY"))
        .and_then(|l| l.rsplit_once('['))
        .map_or_else(String::new, |(_, rest)| {
            rest.trim_end_matches(']').to_string()
        })
}

#[test]
fn tool_hooks_never_create_the_database() {
    let sb = Sandbox::new("nodb");
    let cwd = sb.project().display().to_string();
    assert_eq!(
        sb.hook(&failure("s1", &cwd, "pkill -f x", "Exit code 144"))
            .trim(),
        ""
    );
    assert!(!sb.root.join("data/saispace.db").exists());
}
