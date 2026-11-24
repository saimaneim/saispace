use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("sai-cli-{tag}-{n}"));
        fs::create_dir_all(root.join("repo/.git")).expect("mkdir repo");
        Self { root }
    }

    fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.repo().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, text).expect("write");
    }

    fn sai(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sai"))
            .args(args)
            .current_dir(cwd)
            .env("SAISPACE_DB_PATH", self.root.join("db/saispace.db"))
            .env("SAISPACE_AGENT_NAME", "cli-test")
            .output()
            .expect("run sai")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn rule_add_check_context_and_exit_codes() {
    let sb = Sandbox::new("rules");
    let repo = sb.repo();
    sb.write(
        "src/lib.rs",
        "fn a() { x.unwrap(); }\n#[cfg(test)]\nmod t { fn b() { y.unwrap(); } }\n",
    );
    sb.write("src/ok.rs", "fn ok() {}\n");

    let added = sb.sai(
        &repo.join("src"),
        &[
            "rule",
            "add",
            "--id",
            "no-unwrap",
            "--description",
            "No unwrap",
            "--files",
            "src/**/*.rs",
            "--forbid",
            r"\.unwrap\(\)",
            "--stop-at",
            r"^#\[cfg\(test\)\]",
            "--fix",
            "usa ?",
            "--guide",
            "Errores tipados",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    assert!(stdout(&added).contains("1 violaciones actuales"));
    assert!(repo.join(".saispace/rules/no-unwrap.toml").is_file());

    let dup = sb.sai(
        &repo,
        &[
            "rule",
            "add",
            "--id",
            "no-unwrap",
            "--description",
            "x",
            "--files",
            "*",
            "--forbid",
            "x",
        ],
    );
    assert_eq!(dup.status.code(), Some(2));

    let bad = sb.sai(
        &repo,
        &[
            "rule",
            "add",
            "--id",
            "bad",
            "--description",
            "x",
            "--files",
            "*",
            "--forbid",
            "(",
        ],
    );
    assert_eq!(bad.status.code(), Some(2));
    assert!(!repo.join(".saispace/rules/bad.toml").exists());

    let failed = sb.sai(&repo, &["check"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(stdout(&failed).contains("src/lib.rs:1:11  error  [no-unwrap] No unwrap"));

    let json = sb.sai(&repo, &["check", "--json", "src/ok.rs"]);
    assert!(json.status.success());
    let report: serde_json::Value = serde_json::from_slice(&json.stdout).expect("json");
    assert_eq!(report["findings"].as_array().map(Vec::len), Some(0));

    sb.write(
        "src/lib.rs",
        "fn a() { x.unwrap(); } // sai-ignore: no-unwrap\n",
    );
    assert!(sb.sai(&repo, &["check"]).status.success());

    let ctx = sb.sai(&repo, &["context", "src/lib.rs", "README.md"]);
    let text = stdout(&ctx);
    assert!(text.contains("[no-unwrap] No unwrap"));
    assert!(text.contains("Errores tipados"));
    assert!(text.contains("README.md\n  sin reglas"));

    assert!(sb.sai(&repo, &["rule", "rm", "no-unwrap"]).status.success());
    assert_eq!(
        sb.sai(&repo, &["rule", "rm", "no-unwrap"]).status.code(),
        Some(2)
    );
}

#[test]
fn memory_save_upsert_search_context_and_validation() {
    let sb = Sandbox::new("memory");
    let repo = sb.repo();

    let saved = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Decisión de auth",
            "--content",
            "Usamos cookies firmadas",
            "--kind",
            "decision",
            "--topic",
            "arch/auth",
        ],
    );
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );

    let again = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Decisión de auth",
            "--content",
            "Pasamos a tokens rotativos",
            "--topic",
            "arch/auth",
        ],
    );
    assert!(again.status.success());

    let ctx = sb.sai(&repo, &["memory", "context", "--json"]);
    let context: serde_json::Value = serde_json::from_slice(&ctx.stdout).expect("json");
    let items = context["memories"].as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["content"], "Pasamos a tokens rotativos");
    assert_eq!(items[0]["kind"], "note");

    let index = stdout(&sb.sai(&repo, &["memory", "context"]));
    assert!(index.starts_with("# Memoria de "), "{index}");
    assert!(index.contains("- [note] Decisión de auth · "), "{index}");
    assert!(
        !index.contains("Pasamos a tokens"),
        "the index lists titles only"
    );
    let id = items[0]["id"].as_str().expect("id");
    let full = stdout(&sb.sai(&repo, &["memory", "get", id]));
    assert!(full.starts_with("# Decisión de auth\nnote · "), "{full}");
    assert!(full.contains("Pasamos a tokens rotativos"));

    let search = sb.sai(&repo, &["memory", "search", "rotativ", "--json"]);
    let hits: serde_json::Value = serde_json::from_slice(&search.stdout).expect("json");
    assert_eq!(hits[0]["kind"], "memory");

    let empty_query = sb.sai(&repo, &["memory", "search", "--", "--"]);
    assert_eq!(empty_query.status.code(), Some(2));
    let bad_kind = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "t",
            "--content",
            "c",
            "--kind",
            "nope",
        ],
    );
    assert_eq!(bad_kind.status.code(), Some(2));

    assert!(sb.sai(&repo, &["memory", "rm", id]).status.success());
    assert_eq!(sb.sai(&repo, &["memory", "get", id]).status.code(), Some(2));
}

fn sai_with_stdin(sb: &Sandbox, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sai"))
        .args(args)
        .current_dir(sb.repo())
        .env("SAISPACE_DB_PATH", sb.root.join("db/saispace.db"))
        .env("SAISPACE_AGENT_NAME", "cli-test")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn sai");
    std::io::Write::write_all(&mut child.stdin.take().expect("stdin"), input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait sai")
}

#[test]
fn secret_run_needs_approval_and_masks_value() {
    let sb = Sandbox::new("secret");
    let repo = sb.repo();
    let set = sai_with_stdin(
        &sb,
        &["secret", "set", "DEMO_TOKEN", "--description", "demo"],
        "tok-123456\n",
    );
    assert_eq!(
        set.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );

    let denied = sb.sai(
        &repo,
        &[
            "secret",
            "run",
            "-s",
            "DEMO_TOKEN=TOK",
            "--reason",
            "test",
            "--",
            "sh",
            "-c",
            "echo $TOK",
        ],
    );
    assert_eq!(denied.status.code(), Some(3));
    assert!(!stdout(&denied).contains("tok-123456"));

    let listed = stdout(&sb.sai(&repo, &["secret", "list"]));
    assert!(listed.contains("pendiente"), "{listed}");
    assert!(!listed.contains("tok-123456"));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    runtime.block_on(async {
        let db = saispace_core::Db::open(&sb.root.join("db/saispace.db"))
            .await
            .expect("db");
        let grants = db.list_secret_grants().await.expect("grants");
        assert_eq!(grants[0].reason, "test");
        db.decide_secret_grant(grants[0].id, true)
            .await
            .expect("approve");
    });

    let echoed = sb.sai(
        &repo,
        &[
            "secret",
            "run",
            "-s",
            "DEMO_TOKEN=TOK",
            "--",
            "sh",
            "-c",
            "echo value=$TOK; exit 7",
        ],
    );
    assert_eq!(echoed.status.code(), Some(7));
    let text = stdout(&echoed);
    assert!(text.contains("value=••••••"), "{text}");
    assert!(!text.contains("tok-123456"));

    let compared = sb.sai(
        &repo,
        &[
            "secret",
            "run",
            "-s",
            "DEMO_TOKEN",
            "--",
            "sh",
            "-c",
            "test \"$DEMO_TOKEN\" = tok-123456",
        ],
    );
    assert_eq!(compared.status.code(), Some(0));
}
