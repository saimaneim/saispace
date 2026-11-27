use saispace_rules::{
    Check, Engine, RuleDef, RulesError, Severity, find_root, load_rules, remove_rule, write_rule,
};
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("sai-rules-{n}-{}", std::process::id()));
        fs::create_dir_all(dir.join(".git")).expect("create temp repo");
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, text).expect("write fixture");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn forbid(id: &str, files: &[&str], pattern: &str) -> RuleDef {
    RuleDef {
        id: id.to_string(),
        description: format!("{id} is forbidden"),
        severity: Severity::Error,
        files: files.iter().map(|s| (*s).to_string()).collect(),
        exclude: Vec::new(),
        only_if: None,
        unless: None,
        check: Some(Check::Forbid {
            pattern: pattern.to_string(),
            stop_at: None,
            multiline: false,
            unless_line: None,
            except: None,
            within: None,
            unless_span: None,
        }),
        fix: Some("use ? instead".to_string()),
        guide: None,
    }
}

#[test]
fn forbid_reports_line_column_and_respects_stop_at_and_suppression() {
    let mut rule = forbid("no-unwrap", &["**/*.rs"], r"\.unwrap\(\)");
    rule.check = Some(Check::Forbid {
        pattern: r"\.unwrap\(\)".to_string(),
        stop_at: Some(r"^#\[cfg\(test\)\]".to_string()),
        multiline: false,
        unless_line: None,
        except: None,
        within: None,
        unless_span: None,
    });
    let engine = Engine::new(&[rule]).expect("engine");
    let text = "fn a() {\n    x.unwrap();\n    y.unwrap(); // sai-ignore: no-unwrap\n    // sai-ignore: other, no-unwrap\n    z.unwrap();\n}\n#[cfg(test)]\nmod t { fn b() { q.unwrap(); } }\n";
    let findings = engine.check_text("src/lib.rs", text);
    assert_eq!(findings.len(), 1);
    assert_eq!((findings[0].line, findings[0].column), (2, 6));
    assert_eq!(findings[0].fix.as_deref(), Some("use ? instead"));
    assert_eq!(findings[0].snippet, "x.unwrap();");
}

#[test]
fn require_and_max_lines_checks() {
    let require = RuleDef {
        id: "has-license".to_string(),
        description: "needs SPDX header".to_string(),
        severity: Severity::Warn,
        files: vec!["*.ts".to_string()],
        exclude: vec!["gen.ts".to_string()],
        only_if: None,
        unless: None,
        check: Some(Check::Require {
            pattern: "SPDX-License-Identifier".to_string(),
        }),
        fix: None,
        guide: None,
    };
    let max = RuleDef {
        id: "short-files".to_string(),
        description: "keep files short".to_string(),
        severity: Severity::Error,
        files: vec!["*.ts".to_string()],
        exclude: Vec::new(),
        only_if: None,
        unless: None,
        check: Some(Check::MaxLines { max: 3 }),
        fix: None,
        guide: None,
    };
    let engine = Engine::new(&[require, max]).expect("engine");
    let found = engine.check_text("a.ts", "1\n2\n3\n4\n");
    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|f| f.rule == "short-files" && f.line == 4));
    assert!(
        found
            .iter()
            .any(|f| f.rule == "has-license" && f.severity == Severity::Warn)
    );
    assert_eq!(engine.check_text("gen.ts", "x").len(), 0);
}

#[test]
fn invalid_definitions_are_typed_errors() {
    let bad_regex = forbid("bad", &["*"], "(");
    assert!(matches!(
        Engine::new(&[bad_regex]),
        Err(RulesError::Regex { .. })
    ));
    let bad_glob = forbid("bad", &["a/{b"], "x");
    assert!(matches!(
        Engine::new(&[bad_glob]),
        Err(RulesError::Glob { .. })
    ));
    let bad_id = forbid("Bad Id", &["*"], "x");
    assert!(matches!(
        Engine::new(&[bad_id]),
        Err(RulesError::Invalid(..))
    ));
    let mut empty = forbid("empty", &["*"], "x");
    empty.check = None;
    assert!(matches!(
        Engine::new(&[empty]),
        Err(RulesError::Invalid(..))
    ));
}

#[test]
fn store_roundtrip_and_tree_scan_respect_gitignore() {
    let repo = TempDir::new();
    repo.write(".gitignore", "ignored/\n");
    repo.write("src/a.rs", "fn a() { x.unwrap(); }\n");
    repo.write("src/nested/b.rs", "fn b() {}\n");
    repo.write("ignored/c.rs", "fn c() { x.unwrap(); }\n");
    repo.write("other/d.rs", "fn d() { x.unwrap(); }\n");

    let mut rule = forbid("no-unwrap", &["src/**/*.rs"], r"\.unwrap\(\)");
    rule.guide = Some("Propagate errors with ?".to_string());
    write_rule(repo.path(), &rule, false).expect("write");
    assert!(matches!(
        write_rule(repo.path(), &rule, false),
        Err(RulesError::Duplicate(_))
    ));
    write_rule(repo.path(), &rule, true).expect("force replace");

    let nested = repo.path().join("src/nested");
    assert_eq!(find_root(&nested), repo.path());

    let rules = load_rules(repo.path()).expect("load");
    assert_eq!(rules, vec![rule]);

    let engine = Engine::new(&rules).expect("engine");
    let report = engine.check_tree(repo.path(), &[]);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].path, "src/a.rs");
    assert_eq!(report.errors(), 1);
    assert_eq!(report.files_scanned, 2);
    assert_eq!(engine.rules_for("src/x.rs").len(), 1);
    assert_eq!(engine.rules_for("docs/x.md").len(), 0);

    let scoped = engine.check_tree(repo.path(), &[nested]);
    assert_eq!(scoped.findings.len(), 0);

    repo.write(".agents/checks/broken.toml", "id = \"broken\"\n");
    assert!(matches!(
        load_rules(repo.path()),
        Err(RulesError::Parse { .. })
    ));
    fs::remove_file(repo.path().join(".agents/checks/broken.toml")).expect("cleanup");

    remove_rule(repo.path(), "no-unwrap").expect("remove");
    assert!(matches!(
        remove_rule(repo.path(), "no-unwrap"),
        Err(RulesError::NotFound(_))
    ));
}
