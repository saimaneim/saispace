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

    let pinned = sb.sai(&repo, &["memory", "pin", id]);
    assert!(pinned.status.success());
    assert!(stdout(&pinned).contains(&format!("fijada {id}")));

    let pinned_ctx = stdout(&sb.sai(&repo, &["memory", "context"]));
    assert!(pinned_ctx.contains("## Fijadas"));
    assert!(pinned_ctx.contains(&format!("- [note] Decisión de auth · {id}")));
    assert!(pinned_ctx.contains("Pasamos a tokens rotativos"));

    let pinned_json = sb.sai(&repo, &["memory", "context", "--json"]);
    let pinned_obj: serde_json::Value = serde_json::from_slice(&pinned_json.stdout).expect("json");
    assert_eq!(pinned_obj["pinned"].as_array().expect("array").len(), 1);
    assert_eq!(pinned_obj["memories"].as_array().expect("array").len(), 0);

    let unpinned = sb.sai(&repo, &["memory", "unpin", id]);
    assert!(unpinned.status.success());
    assert!(stdout(&unpinned).contains(&format!("desfijada {id}")));

    let unpinned_ctx = stdout(&sb.sai(&repo, &["memory", "context"]));
    assert!(!unpinned_ctx.contains("## Fijadas"));

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

    let rm = sb.sai(&repo, &["memory", "rm", id]);
    assert!(rm.status.success());
    assert!(stdout(&rm).contains(&format!("eliminada {id}  Decisión de auth")));
    assert_eq!(sb.sai(&repo, &["memory", "get", id]).status.code(), Some(2));
}

#[test]
fn memory_list_grouping_and_filtering() {
    let sb = Sandbox::new("memory-list-grp");
    let repo = sb.repo();

    let empty = sb.sai(&repo, &["memory", "list"]);
    assert!(empty.status.success());
    assert_eq!(stdout(&empty).trim(), "Sin memorias.");

    let dec = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Decisión SQLite",
            "--content",
            "Usar SQLite embebido",
            "--kind",
            "decision",
        ],
    );
    assert!(dec.status.success());

    let note = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Nota general",
            "--content",
            "Información adicional",
            "--kind",
            "note",
        ],
    );
    assert!(note.status.success());

    let listed = sb.sai(&repo, &["memory", "list"]);
    assert!(listed.status.success());
    let list_txt = stdout(&listed);
    assert!(list_txt.starts_with("# Memorias de repo · 2"), "{list_txt}");
    assert!(list_txt.contains("## decision"), "{list_txt}");
    assert!(list_txt.contains("## note"), "{list_txt}");
    assert!(list_txt.contains("Decisión SQLite"), "{list_txt}");
    assert!(list_txt.contains("Nota general"), "{list_txt}");

    let filtered = sb.sai(&repo, &["memory", "list", "--kind", "decision"]);
    assert!(filtered.status.success());
    let filtered_txt = stdout(&filtered);
    assert!(
        filtered_txt.starts_with("# Memorias de repo · 1"),
        "{filtered_txt}"
    );
    assert!(filtered_txt.contains("## decision"), "{filtered_txt}");
    assert!(filtered_txt.contains("Decisión SQLite"), "{filtered_txt}");
    assert!(!filtered_txt.contains("## note"), "{filtered_txt}");
    assert!(!filtered_txt.contains("Nota general"), "{filtered_txt}");

    let bad_kind = sb.sai(&repo, &["memory", "list", "--kind", "inventado"]);
    assert_eq!(bad_kind.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad_kind.stderr).contains("unknown memory kind 'inventado'"));
}

#[test]
fn memory_list_scoping_and_cross_project() {
    let sb = Sandbox::new("memory-list-scope");
    let repo = sb.repo();

    let saved_local = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Memoria local",
            "--content",
            "Local repo",
            "--kind",
            "note",
        ],
    );
    assert!(saved_local.status.success());

    let repo2 = sb.root.join("repo2");
    fs::create_dir_all(repo2.join(".git")).expect("mkdir repo2");
    let saved_other = sb.sai(
        &repo2,
        &[
            "memory",
            "save",
            "--title",
            "Memoria de repo2",
            "--content",
            "Contenido repo2",
            "--kind",
            "pattern",
        ],
    );
    assert!(saved_other.status.success());

    let list_local = stdout(&sb.sai(&repo, &["memory", "list"]));
    assert!(!list_local.contains("Memoria de repo2"), "{list_local}");

    let list_all = stdout(&sb.sai(&repo, &["memory", "list", "--all"]));
    assert!(list_all.contains("Memoria de repo2"), "{list_all}");
    assert!(list_all.contains("Memoria local"), "{list_all}");

    let saved_global = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--global",
            "--title",
            "Memoria global",
            "--content",
            "Aplica en cualquier lugar",
            "--kind",
            "preference",
        ],
    );
    assert!(saved_global.status.success());

    let plain_dir = sb.root.join("plain_dir");
    fs::create_dir_all(&plain_dir).expect("mkdir plain_dir");
    let global_list = sb.sai(&plain_dir, &["memory", "list"]);
    assert!(global_list.status.success());
    let global_txt = stdout(&global_list);
    assert!(
        global_txt.starts_with("# Memorias globales · 1"),
        "{global_txt}"
    );
    assert!(global_txt.contains("## preference"), "{global_txt}");
    assert!(global_txt.contains("Memoria global"), "{global_txt}");
    assert!(!global_txt.contains("Memoria local"), "{global_txt}");
    assert!(!global_txt.contains("Memoria de repo2"), "{global_txt}");

    let json_res = sb.sai(&repo, &["memory", "list", "--json"]);
    assert!(json_res.status.success());
    let json_val: serde_json::Value = serde_json::from_slice(&json_res.stdout).expect("json");
    let arr = json_val.as_array().expect("array");
    assert_eq!(arr.len(), 2);

    let saved_for_rm = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Borrame luego",
            "--content",
            "Temporal",
            "--kind",
            "note",
        ],
    );
    assert!(saved_for_rm.status.success());
    let rm_json = sb.sai(&repo, &["memory", "list", "--json"]);
    let rm_val: serde_json::Value = serde_json::from_slice(&rm_json.stdout).expect("json");
    let target = rm_val
        .as_array()
        .expect("array")
        .iter()
        .find(|m| m["title"] == "Borrame luego")
        .expect("found memory");
    let target_id = target["id"].as_str().expect("id");
    let rm_res = sb.sai(&repo, &["memory", "rm", target_id]);
    assert!(rm_res.status.success());
    let rm_txt = stdout(&rm_res);
    assert!(
        rm_txt.contains(&format!("eliminada {target_id}  Borrame luego")),
        "{rm_txt}"
    );
}

fn sai_with_stdin(sb: &Sandbox, args: &[&str], input: &str) -> Output {
    sai_with_stdin_in(sb, &sb.repo(), args, input)
}

fn sai_with_stdin_in(sb: &Sandbox, cwd: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sai"))
        .args(args)
        .current_dir(cwd)
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

#[test]
fn memory_edit_updates_fields_and_validates_input() {
    let sb = Sandbox::new("mem-edit");
    let repo = sb.repo();

    let save = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Original",
            "--content",
            "Contenido inicial",
            "--kind",
            "note",
            "--topic",
            "arch/orig",
        ],
    );
    assert!(save.status.success());
    let list = sb.sai(&repo, &["memory", "context", "--json"]);
    let ctx: serde_json::Value = serde_json::from_slice(&list.stdout).expect("json");
    let id = ctx["memories"][0]["id"].as_str().expect("id");

    let no_fields = sb.sai(&repo, &["memory", "edit", id]);
    assert_eq!(no_fields.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&no_fields.stderr).contains("indica al menos un campo"));

    let edit = sb.sai(
        &repo,
        &[
            "memory",
            "edit",
            id,
            "--title",
            "Nuevo titulo",
            "--kind",
            "bugfix",
            "--topic",
            "arch/nuevo",
        ],
    );
    assert!(
        edit.status.success(),
        "{}",
        String::from_utf8_lossy(&edit.stderr)
    );
    let edit_out = stdout(&edit);
    assert!(edit_out.contains("Nuevo titulo"));
    assert!(edit_out.contains("Bugfix"));

    let get = sb.sai(&repo, &["memory", "get", id, "--json"]);
    let mem: serde_json::Value = serde_json::from_slice(&get.stdout).expect("json");
    assert_eq!(mem["title"], "Nuevo titulo");
    assert_eq!(mem["kind"], "bugfix");
    assert_eq!(mem["topic_key"], "arch-nuevo");
    assert_eq!(mem["content"], "Contenido inicial");

    let stdin_edit = sai_with_stdin(
        &sb,
        &["memory", "edit", id, "--content", "-"],
        "Contenido stdin\n",
    );
    assert!(stdin_edit.status.success());
    let get2 = sb.sai(&repo, &["memory", "get", id, "--json"]);
    let mem2: serde_json::Value = serde_json::from_slice(&get2.stdout).expect("json");
    assert_eq!(mem2["content"], "Contenido stdin");

    let clear_topic = sb.sai(&repo, &["memory", "edit", id, "--topic", ""]);
    assert!(clear_topic.status.success());
    let get3 = sb.sai(&repo, &["memory", "get", id, "--json"]);
    let mem3: serde_json::Value = serde_json::from_slice(&get3.stdout).expect("json");
    assert!(mem3["topic_key"].is_null());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    runtime.block_on(async {
        let db = saispace_core::Db::open(&sb.root.join("db/saispace.db"))
            .await
            .expect("db");
        let activity = db.list_activity(10).await.expect("activity");
        let update_event = activity.iter().find(|e| {
            e.action == saispace_core::models::ActionKind::Update
                && e.entity == saispace_core::models::EntityKind::Memory
        });
        assert!(update_event.is_some(), "Update activity event was recorded");
    });
}

#[test]
fn memory_save_normalizes_topic_and_hints_similar() {
    let sb = Sandbox::new("mem-topic");
    let repo = sb.repo();

    let first = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Reglas del panel de UI",
            "--content",
            "Paleta estilo Vercel",
            "--topic",
            "arch/auth",
        ],
    );
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    // Same title saved without a topic: the CLI points at the topic the
    // similar memory already has.
    let twin = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Reglas panel UI v2",
            "--content",
            "Duplicada",
        ],
    );
    assert!(twin.status.success());
    let twin_out = stdout(&twin);
    assert!(
        twin_out.contains("usa --topic arch-auth para actualizarla"),
        "{twin_out}"
    );

    // A messy key is stored with the convention.
    let saved = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Exportación PDF",
            "--content",
            "Usamos printcss",
            "--topic",
            "Proj/My_Topic",
        ],
    );
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );
    let list = sb.sai(&repo, &["memory", "list", "--json"]);
    let memories: serde_json::Value = serde_json::from_slice(&list.stdout).expect("json");
    let row = memories
        .as_array()
        .expect("array")
        .iter()
        .find(|m| m["title"] == "Exportación PDF")
        .expect("saved row");
    assert_eq!(row["topic_key"], "proj-my-topic");
}

#[test]
fn memory_orphans_and_linking() {
    let sb = Sandbox::new("orphans");
    let repo = sb.repo();

    let initial = stdout(&sb.sai(&repo, &["memory", "orphans"]));
    assert!(initial.contains("sin memorias huérfanas"));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    runtime.block_on(async {
        let db = saispace_core::Db::open(&sb.root.join("db/saispace.db"))
            .await
            .expect("db");
        db.save_memory(saispace_core::models::SaveMemoryInput {
            project_id: None,
            project_name: Some("imported-repo".to_string()),
            kind: saispace_core::models::MemoryKind::Decision,
            title: "Imported auth strategy".to_string(),
            content: "JWT stateless tokens".to_string(),
            topic_key: None,
        })
        .await
        .expect("save");
    });

    let listed = stdout(&sb.sai(&repo, &["memory", "orphans"]));
    assert!(listed.contains("imported-repo"));
    assert!(listed.contains("1 memoria"));

    let linked = stdout(&sb.sai(&repo, &["memory", "link", "imported-repo"]));
    assert!(linked.contains("1 memorias vinculadas a repo"));

    let after = stdout(&sb.sai(&repo, &["memory", "orphans"]));
    assert!(after.contains("sin memorias huérfanas"));

    let ctx = sb.sai(&repo, &["memory", "context", "--json"]);
    let context: serde_json::Value = serde_json::from_slice(&ctx.stdout).expect("json");
    let items = context["memories"].as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Imported auth strategy");
}

#[test]
fn memory_dupes_detects_and_prints_pairs() {
    let sb = Sandbox::new("memory-dupes");
    let repo = sb.repo();

    let initial = sb.sai(&repo, &["memory", "dupes"]);
    assert_eq!(initial.status.code(), Some(0));
    assert!(stdout(&initial).contains("sin duplicados"));

    let m1 = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Database connection pooling configuration",
            "--content",
            "Usar 5 conexiones",
        ],
    );
    assert_eq!(m1.status.code(), Some(0));

    let m2 = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Database connection pooling settings",
            "--content",
            "Max 5 conexiones sqlite",
        ],
    );
    assert_eq!(m2.status.code(), Some(0));

    let dupes = sb.sai(&repo, &["memory", "dupes"]);
    assert_eq!(dupes.status.code(), Some(0));
    let out = stdout(&dupes);
    assert!(!out.contains("sin duplicados"));
    assert!(out.contains("Database connection pooling"));
    assert!(out.contains("≈"));
}

fn find_memory_by_title(sb: &Sandbox, repo: &Path, title: &str) -> serde_json::Value {
    let out = sb.sai(repo, &["memory", "list", "--json"]);
    assert!(out.status.success());
    let items: serde_json::Value = serde_json::from_slice(&out.stdout).expect("list json");
    items
        .as_array()
        .expect("array")
        .iter()
        .find(|m| m["title"] == title)
        .cloned()
        .expect("memory found")
}

#[test]
fn memory_usage_tracks_get_search_and_ignores_list_and_context() {
    let sb = Sandbox::new("memory-usage");
    let repo = sb.repo();

    for args in [
        &[
            "memory",
            "save",
            "--global",
            "--kind",
            "preference",
            "--title",
            "Preferencia global",
            "--content",
            "Usar biome siempre",
        ][..],
        &[
            "memory",
            "save",
            "--kind",
            "decision",
            "--title",
            "Cola eventos",
            "--content",
            "Token unico usage_cli_marker",
        ][..],
        &[
            "memory",
            "save",
            "--kind",
            "note",
            "--title",
            "Fijada",
            "--content",
            "Detalle importante fijado",
        ][..],
    ] {
        assert!(sb.sai(&repo, args).status.success());
    }

    let decision = find_memory_by_title(&sb, &repo, "Cola eventos");
    let decision_id = decision["id"].as_str().expect("id");
    let pinned_mem = find_memory_by_title(&sb, &repo, "Fijada");
    let pinned_id = pinned_mem["id"].as_str().expect("pinned id");
    assert!(
        sb.sai(&repo, &["memory", "pin", pinned_id])
            .status
            .success()
    );

    let listed_again = find_memory_by_title(&sb, &repo, "Cola eventos");
    assert_eq!(listed_again["use_count"], 0);
    assert!(listed_again["last_used_at"].is_null());

    assert!(
        sb.sai(&repo, &["memory", "get", decision_id])
            .status
            .success()
    );
    assert!(
        sb.sai(&repo, &["memory", "get", decision_id])
            .status
            .success()
    );

    let get_json_out = sb.sai(&repo, &["memory", "get", decision_id, "--json"]);
    assert!(get_json_out.status.success());
    let get_json: serde_json::Value =
        serde_json::from_slice(&get_json_out.stdout).expect("get json");
    assert_eq!(get_json["use_count"], 2);
    assert!(get_json["last_used_at"].is_string());
    assert_eq!(get_json["updated_at"], decision["updated_at"]);

    assert!(
        sb.sai(&repo, &["memory", "search", "usage_cli_marker"])
            .status
            .success()
    );
    assert_eq!(
        find_memory_by_title(&sb, &repo, "Cola eventos")["use_count"],
        4
    );
    assert_eq!(find_memory_by_title(&sb, &repo, "Fijada")["use_count"], 0);

    // The session-start index only lists memories: it does not count as use.
    assert!(sb.sai(&repo, &["memory", "context"]).status.success());
    assert_eq!(
        find_memory_by_title(&sb, &repo, "Cola eventos")["use_count"],
        4
    );
    assert_eq!(find_memory_by_title(&sb, &repo, "Fijada")["use_count"], 0);
    assert_eq!(
        find_memory_by_title(&sb, &repo, "Preferencia global")["use_count"],
        0
    );
}

#[test]
fn memory_context_compact_notice_via_stdin_and_flag() {
    let sb = Sandbox::new("ctx-compact");
    let repo = sb.repo();

    let saved = sb.sai(
        &repo,
        &[
            "memory",
            "save",
            "--title",
            "Una decisión",
            "--content",
            "Contenido de prueba",
            "--kind",
            "decision",
        ],
    );
    assert!(saved.status.success());

    // Piped compact -> notice prepended before the index.
    let compact = sai_with_stdin(&sb, &["memory", "context"], r#"{"source":"compact"}"#);
    assert!(compact.status.success());
    let compact_txt = stdout(&compact);
    assert!(
        compact_txt.contains("## Primera acción tras compactar"),
        "{compact_txt}"
    );
    assert!(compact_txt.contains("session_summary"), "{compact_txt}");
    assert!(compact_txt.contains("# Memoria de "), "{compact_txt}");

    // Piped startup -> no notice.
    let startup = sai_with_stdin(&sb, &["memory", "context"], r#"{"source":"startup"}"#);
    let startup_txt = stdout(&startup);
    assert!(
        !startup_txt.contains("Primera acción tras compactar"),
        "{startup_txt}"
    );
    assert!(startup_txt.starts_with("# Memoria de "), "{startup_txt}");

    // Invalid stdin is ignored: the index still renders, no notice.
    let invalid = sai_with_stdin(&sb, &["memory", "context"], "esto no es json");
    assert!(invalid.status.success());
    let invalid_txt = stdout(&invalid);
    assert!(invalid_txt.starts_with("# Memoria de "), "{invalid_txt}");
    assert!(
        !invalid_txt.contains("Primera acción tras compactar"),
        "{invalid_txt}"
    );

    // Explicit --source compact flag -> notice present.
    let flag = sb.sai(&repo, &["memory", "context", "--source", "compact"]);
    assert!(flag.status.success());
    assert!(stdout(&flag).contains("## Primera acción tras compactar"));

    // --json with compact adds a "notice" field.
    let json = sai_with_stdin(
        &sb,
        &["memory", "context", "--json"],
        r#"{"source":"compact"}"#,
    );
    let json_val: serde_json::Value = serde_json::from_slice(&json.stdout).expect("json");
    let notice = json_val["notice"].as_str().expect("notice field");
    assert!(notice.contains("Primera acción tras compactar"), "{notice}");

    // --json without compact has no notice field.
    let plain_json = sb.sai(&repo, &["memory", "context", "--json"]);
    let plain_val: serde_json::Value = serde_json::from_slice(&plain_json.stdout).expect("json");
    assert!(plain_val.get("notice").is_none());
}

#[test]
fn memory_context_cwd_from_json_selects_project() {
    let sb = Sandbox::new("ctx-cwd");
    let repo = sb.repo();

    let repo2 = sb.root.join("project-b");
    fs::create_dir_all(repo2.join(".git")).expect("mkdir repo2");

    let saved = sb.sai(
        &repo2,
        &[
            "memory",
            "save",
            "--title",
            "Memoria de project-b",
            "--content",
            "Contenido del otro proyecto",
            "--kind",
            "note",
        ],
    );
    assert!(saved.status.success());

    // Running from `repo` but with cwd pointing at project-b must resolve
    // project-b, not the process cwd.
    let input = format!(r#"{{"source":"startup","cwd":"{}"}}"#, repo2.display());
    let out = sai_with_stdin_in(&sb, &repo, &["memory", "context"], &input);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let txt = stdout(&out);
    assert!(txt.contains("# Memoria de project-b"), "{txt}");
    assert!(txt.contains("Memoria de project-b"), "{txt}");
    assert!(!txt.contains("# Memoria global"), "{txt}");
}

fn seed_stale_memories(sb: &Sandbox, repo: &Path) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    runtime.block_on(async {
        let db = saispace_core::Db::open(&sb.root.join("db/saispace.db"))
            .await
            .expect("db");
        let project = db
            .project_for_path(&repo.canonicalize().expect("canonical repo"), false)
            .await
            .expect("project lookup")
            .expect("project registered");
        let now = chrono::Utc::now();
        let entries: &[(
            saispace_core::models::MemoryKind,
            &str,
            Option<uuid::Uuid>,
            i64,
        )] = &[
            (
                saispace_core::models::MemoryKind::Decision,
                "Decisión olvidada",
                Some(project.id),
                100,
            ),
            (
                saispace_core::models::MemoryKind::Preference,
                "Preferencia vieja",
                Some(project.id),
                200,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión proyecto 1",
                Some(project.id),
                1,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión proyecto 2",
                Some(project.id),
                2,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión proyecto 3",
                Some(project.id),
                3,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión proyecto 4",
                Some(project.id),
                4,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión global 1",
                None,
                1,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión global 2",
                None,
                2,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión global 3",
                None,
                3,
            ),
            (
                saispace_core::models::MemoryKind::SessionSummary,
                "Sesión global 4",
                None,
                4,
            ),
        ];
        for (kind, title, project_id, age_days) in entries {
            let ts = now - chrono::Duration::days(*age_days);
            db.save_memory_at(
                saispace_core::models::SaveMemoryInput {
                    project_id: *project_id,
                    project_name: None,
                    kind: *kind,
                    title: (*title).to_string(),
                    content: "Contenido de prueba".to_string(),
                    topic_key: None,
                },
                ts,
                Some(ts),
            )
            .await
            .expect("save old memory");
        }
    });
}

fn stale_json(sb: &Sandbox, repo: &Path, extra: &[&str]) -> serde_json::Value {
    let mut args = vec!["memory", "stale", "--json"];
    args.extend_from_slice(extra);
    let out = sb.sai(repo, &args);
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).expect("stale json")
}

fn memory_delete_events(sb: &Sandbox) -> usize {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    runtime.block_on(async {
        let db = saispace_core::Db::open(&sb.root.join("db/saispace.db"))
            .await
            .expect("db");
        let activity = db.list_activity(20).await.expect("activity");
        activity
            .iter()
            .filter(|e| {
                e.action == saispace_core::models::ActionKind::Delete
                    && e.entity == saispace_core::models::EntityKind::Memory
            })
            .count()
    })
}

#[test]
fn memory_stale_lists_and_prunes_only_surplus_sessions() {
    let sb = Sandbox::new("memory-stale");
    let repo = sb.repo();

    let initial = sb.sai(&repo, &["memory", "stale"]);
    assert_eq!(initial.status.code(), Some(0));
    assert!(stdout(&initial).contains("nada que revisar"));

    // A fresh memory registers the project and must never show up as stale.
    assert!(
        sb.sai(
            &repo,
            &[
                "memory",
                "save",
                "--title",
                "Nota reciente",
                "--content",
                "Al día"
            ],
        )
        .status
        .success()
    );
    seed_stale_memories(&sb, &repo);

    let listed = sb.sai(&repo, &["memory", "stale"]);
    assert_eq!(listed.status.code(), Some(0));
    let out = stdout(&listed);
    assert!(out.contains("Sin uso en 60 días"), "{out}");
    assert!(out.contains("Decisión olvidada"), "{out}");
    assert!(!out.contains("Preferencia vieja"), "{out}");
    assert!(!out.contains("Nota reciente"), "{out}");
    assert!(out.contains("Resúmenes de sesión sobrantes"), "{out}");
    assert!(out.contains("Sesión proyecto 4"), "{out}");
    assert!(out.contains("Sesión global 4"), "{out}");
    assert!(!out.contains("Sesión proyecto 3"), "{out}");

    let report = stale_json(&sb, &repo, &[]);
    assert_eq!(report["stale"].as_array().expect("stale array").len(), 1);
    assert_eq!(
        report["surplus_sessions"]
            .as_array()
            .expect("surplus array")
            .len(),
        2
    );

    let keep1 = stale_json(&sb, &repo, &["--keep-sessions", "1"]);
    assert_eq!(
        keep1["surplus_sessions"]
            .as_array()
            .expect("surplus array")
            .len(),
        6
    );

    let wide = stale_json(&sb, &repo, &["--days", "200"]);
    assert_eq!(wide["stale"].as_array().expect("stale array").len(), 0);

    let pruned = sb.sai(&repo, &["memory", "stale", "--prune-sessions"]);
    assert_eq!(pruned.status.code(), Some(0));
    let pout = stdout(&pruned);
    assert!(pout.contains("eliminada"), "{pout}");
    assert!(pout.contains("Sesión proyecto 4"), "{pout}");
    assert!(pout.contains("Sesión global 4"), "{pout}");
    // Stale memories are listed for review but never auto-deleted.
    assert!(pout.contains("Decisión olvidada"), "{pout}");
    assert_eq!(
        find_memory_by_title(&sb, &repo, "Decisión olvidada")["title"],
        "Decisión olvidada"
    );

    let after = stale_json(&sb, &repo, &[]);
    assert_eq!(after["stale"].as_array().expect("stale array").len(), 1);
    assert_eq!(
        after["surplus_sessions"]
            .as_array()
            .expect("surplus array")
            .len(),
        0
    );
    assert_eq!(memory_delete_events(&sb), 2);
}

#[test]
fn memory_stale_prune_json_stays_parseable() {
    let sb = Sandbox::new("memory-stale-json");
    let repo = sb.repo();
    assert!(
        sb.sai(
            &repo,
            &[
                "memory",
                "save",
                "--title",
                "Nota base",
                "--content",
                "Registra el proyecto"
            ],
        )
        .status
        .success()
    );
    seed_stale_memories(&sb, &repo);

    let pruned = sb.sai(&repo, &["memory", "stale", "--prune-sessions", "--json"]);
    assert_eq!(pruned.status.code(), Some(0));
    let out = stdout(&pruned);
    assert!(!out.contains("eliminada"), "{out}");
    let report: serde_json::Value = serde_json::from_slice(&pruned.stdout).expect("prune json");
    assert_eq!(report["stale"].as_array().expect("stale array").len(), 1);
    assert_eq!(
        report["surplus_sessions"]
            .as_array()
            .expect("surplus array")
            .len(),
        0
    );
    assert_eq!(memory_delete_events(&sb), 2);
}
