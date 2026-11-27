use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("sai-plan-{tag}-{n}"));
        fs::create_dir_all(root.join("repo/.git")).expect("mkdir repo");
        Self { root }
    }

    fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    fn sai(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sai"))
            .args(args)
            .current_dir(self.repo())
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

/// The 8-digit short id `sai plan` prints (first token of a line).
fn short_id_of_line(out: &str, containing: &str) -> String {
    let line = out
        .lines()
        .find(|l| l.contains(containing))
        .unwrap_or_else(|| panic!("line with {containing:?} not found in:\n{out}"));
    line.split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("no short id in line: {line}"))
        .to_string()
}

struct PlanFixture {
    goal: String,
    release: String,
    announce: String,
    inner: String,
    loose: String,
}

/// Creates a goal with two tasks, a loose task and a task inside the goal.
/// The project auto-registers on the first save.
fn create_fixture(sb: &Sandbox) -> PlanFixture {
    let created = sb.sai(&[
        "plan",
        "goal",
        "Publicar la v0",
        "--criteria",
        "Desplegada y anunciada",
        "--task",
        "Preparar release",
        "--task",
        "Anunciar",
    ]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created_out = stdout(&created);
    assert!(created_out.contains("meta"), "{created_out}");
    assert!(created_out.contains("2 tareas"), "{created_out}");
    let goal = short_id_of_line(&created_out, "meta");
    let release = short_id_of_line(&created_out, "Preparar release");
    let announce = short_id_of_line(&created_out, "Anunciar");
    assert_eq!(goal.len(), 8);
    assert_ne!(release, announce);

    let loose = sb.sai(&["plan", "task", "Avisar a clientes"]);
    assert!(loose.status.success(), "{}", stdout(&loose));
    let loose_id = short_id_of_line(&stdout(&loose), "creada");
    let inner = sb.sai(&["plan", "task", "Repasar README", "--goal", &goal]);
    assert!(
        inner.status.success(),
        "{}",
        String::from_utf8_lossy(&inner.stderr)
    );
    let inner_out = stdout(&inner);
    assert!(inner_out.contains(&goal), "{inner_out}");
    let inner_id = short_id_of_line(&inner_out, "creada");

    PlanFixture {
        goal,
        release,
        announce,
        inner: inner_id,
        loose: loose_id,
    }
}

#[test]
fn plan_goal_task_done_complete_flow() {
    let sb = Sandbox::new("flow");
    let fx = create_fixture(&sb);

    // The plan shows the goal with progress and the loose task.
    let plan = sb.sai(&["plan"]);
    assert!(plan.status.success(), "{}", stdout(&plan));
    let plan_out = stdout(&plan);
    assert!(plan_out.contains("# Plan de"), "{plan_out}");
    assert!(
        plan_out.contains(&format!("{} 0/3 Publicar la v0", fx.goal)),
        "{plan_out}"
    );
    assert!(
        plan_out.contains("hecho cuando: Desplegada y anunciada"),
        "{plan_out}"
    );
    assert!(plan_out.contains("[todo] Preparar release"), "{plan_out}");
    assert!(plan_out.contains("[todo] Avisar a clientes"), "{plan_out}");

    // Completing the goal's tasks; the last one prints the hint.
    let done_release = sb.sai(&["plan", "done", &fx.release]);
    assert!(done_release.status.success(), "{}", stdout(&done_release));
    assert!(
        stdout(&done_release).contains("completada"),
        "{}",
        stdout(&done_release)
    );
    assert!(
        !stdout(&done_release).contains("complétalo"),
        "{}",
        stdout(&done_release)
    );
    let done_announce = sb.sai(&["plan", "done", &fx.announce]);
    assert!(done_announce.status.success());
    let done_inner = sb.sai(&["plan", "done", &fx.inner]);
    assert!(done_inner.status.success());
    let done_inner_out = stdout(&done_inner);
    assert!(
        done_inner_out.contains("complétala con: sai plan complete"),
        "{done_inner_out}"
    );

    // Complete the goal; it leaves the active plan.
    let complete = sb.sai(&["plan", "complete", &fx.goal]);
    assert!(complete.status.success(), "{}", stdout(&complete));
    assert!(
        stdout(&complete).contains("completada"),
        "{}",
        stdout(&complete)
    );

    let plan_after = sb.sai(&["plan"]);
    let plan_after_out = stdout(&plan_after);
    assert!(
        !plan_after_out.contains("Publicar la v0"),
        "{plan_after_out}"
    );
    assert!(
        plan_after_out.contains("Tareas sueltas"),
        "{plan_after_out}"
    );
    assert!(
        plan_after_out.contains("Avisar a clientes"),
        "{plan_after_out}"
    );

    // Unknown ids fail cleanly.
    let bad = sb.sai(&["plan", "done", "no-such-task"]);
    assert!(!bad.status.success());
    let bad_complete = sb.sai(&["plan", "complete", "ffffffff"]);
    assert!(!bad_complete.status.success());

    // The loose task still closes.
    let done_loose = sb.sai(&["plan", "done", &fx.loose]);
    assert!(done_loose.status.success(), "{}", stdout(&done_loose));
    let empty = sb.sai(&["plan"]);
    assert!(empty.status.success());
    assert!(
        stdout(&empty).contains("Sin plan activo"),
        "{}",
        stdout(&empty)
    );
}

#[test]
fn plan_outside_project_fails_with_usage() {
    let sb = Sandbox::new("noproj");
    let output = sb.sai(&["plan"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no es un proyecto"), "{stderr}");
}
