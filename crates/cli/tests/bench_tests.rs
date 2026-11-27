use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct BenchSandbox {
    root: PathBuf,
}

impl BenchSandbox {
    fn new(tag: &str) -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("sai-bench-{tag}-{n}"));
        fs::create_dir_all(root.join(".git")).expect("mkdir git");
        Self { root }
    }

    fn copy_fixtures_from_repo(&self) -> PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/fixtures");
        let dst = self.root.join("bench/fixtures");
        copy_dir_all(&src, &dst);
        dst
    }

    fn sai(&self, args: &[&str]) -> std::process::Output {
        let fake_db = self.root.join("never-created/saispace.db");
        Command::new(env!("CARGO_BIN_EXE_sai"))
            .args(args)
            .current_dir(&self.root)
            .env("SAISPACE_DB_PATH", &fake_db)
            .output()
            .expect("run sai")
    }
}

impl Drop for BenchSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("mkdir dst");
    for entry in fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let ty = entry.file_type().expect("file_type");
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

#[test]
fn cli_bench_json_record_and_compare_without_touching_real_db() {
    let sb = BenchSandbox::new("e2e");
    let _fixtures = sb.copy_fixtures_from_repo();

    let json_out = sb.sai(&["bench", "--json", "--record"]);
    assert!(
        json_out.status.success(),
        "{}",
        String::from_utf8_lossy(&json_out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&json_out.stdout).expect("valid json");
    assert_eq!(report["version"], 1);
    assert!(report["git_sha"].is_string());
    assert!(report["date"].is_string());
    let metrics = report["metrics"].as_object().expect("metrics object");
    for key in [
        "correction_precision",
        "correction_recall",
        "correction_f1",
        "stop_accuracy",
        "search_recall_at_5",
        "search_mrr",
        "dupes_precision",
        "dupes_recall",
        "index_bytes",
        "index_tokens_est",
        "latency_context_ms_p50",
        "latency_search_ms_p50",
        "latency_save_ms_p50",
    ] {
        let m = &metrics[key];
        assert!(m["value"].is_number(), "missing value for {key}");
        assert!(m["unit"].is_string(), "missing unit for {key}");
        assert!(
            m["higher_is_better"].is_boolean(),
            "missing higher_is_better for {key}"
        );
        assert!(
            m["informational"].is_boolean(),
            "missing informational for {key}"
        );
    }

    let compare_ok = sb.sai(&["bench", "--compare"]);
    assert_eq!(
        compare_ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&compare_ok.stdout)
    );
    assert!(sb.root.join("bench/history.jsonl").is_file());
    assert!(!sb.root.join("never-created").exists());
}
