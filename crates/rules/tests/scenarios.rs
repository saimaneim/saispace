//! Real-world scenarios for the rules engine: rules written as TOML, as they
//! live in `.saispace/rules/`, run against realistic file contents.

use saispace_rules::{Engine, RuleDef, RulesError, Severity, load_rules, remove_rule};
use std::fs;
use std::path::{Path, PathBuf};

struct TempRepo(PathBuf);

impl TempRepo {
    fn new(tag: &str) -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("sai-rules-{tag}-{n}-{}", std::process::id()));
        fs::create_dir_all(dir.join(".git")).expect("create temp repo");
        Self(dir)
    }

    fn write(&self, rel: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, bytes).expect("write fixture");
    }

    fn rule(&self, id: &str, toml: &str) {
        self.write(&format!(".saispace/rules/{id}.toml"), toml);
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn parse(toml: &str) -> Result<RuleDef, toml::de::Error> {
    toml::from_str(toml)
}

fn engine(tomls: &[&str]) -> Engine {
    let defs: Vec<RuleDef> = tomls
        .iter()
        .map(|t| parse(t).expect("valid toml"))
        .collect();
    Engine::new(&defs).expect("valid rules")
}

const NO_UNWRAP: &str = r#"
id = "rust-no-unwrap"
description = "No .unwrap() in production code"
files = ["crates/*/src/**/*.rs"]
exclude = ["crates/*/src/bin/**"]
fix = "Propagate the error with ?"

[check]
kind = "forbid"
pattern = '\.unwrap\(\)|\.expect\('
stop_at = '^\s*#\[cfg\(test\)\]'
"#;

const NO_TODO: &str = r#"
id = "rust-no-todo"
description = "No todo!()"
severity = "warn"
files = ["**/*.rs"]

[check]
kind = "forbid"
pattern = 'todo!\('
"#;

const LICENSE: &str = r#"
id = "license-header"
description = "Every source file starts with an SPDX header"
severity = "warn"
files = ["**/*.ts"]

[check]
kind = "require"
pattern = '(?m)^// SPDX-License-Identifier: '
"#;

const SHORT: &str = r#"
id = "short-files"
description = "Keep components short"
files = ["src/components/**/*.tsx"]

[check]
kind = "max_lines"
max = 3
"#;

const GUIDE_ONLY: &str = r#"
id = "ui-guide"
description = "UI design guide"
files = ["src/**/*.tsx"]
guide = "Read DESIGN.md before touching components."
"#;

// ---------------------------------------------------------------- not applies

#[test]
fn rule_does_not_apply_outside_its_globs_or_inside_exclude() {
    let engine = engine(&[NO_UNWRAP]);
    let code = "fn main() { x.unwrap(); }\n";

    assert_eq!(engine.check_text("crates/core/src/lib.rs", code).len(), 1);
    // Wrong folder: `crates/*/src/**` does not match top-level `src/`.
    assert_eq!(
        engine.check_text("src/lib.rs", code),
        [] as [saispace_rules::Finding; 0]
    );
    // Excluded binaries.
    assert_eq!(
        engine.check_text("crates/core/src/bin/tool.rs", code),
        [] as [saispace_rules::Finding; 0]
    );
    // Different extension.
    assert_eq!(
        engine.check_text("crates/core/src/lib.ts", code),
        [] as [saispace_rules::Finding; 0]
    );
    assert_eq!(
        engine.rules_for("crates/core/src/bin/tool.rs"),
        [] as [&saispace_rules::RuleDef; 0]
    );
}

#[test]
fn rule_applies_but_finds_nothing_in_clean_code() {
    let engine = engine(&[NO_UNWRAP]);
    let clean = "fn read() -> Result<u8, E> {\n    let v = parse()?;\n    Ok(v)\n}\n";
    assert_eq!(engine.rules_for("crates/core/src/lib.rs").len(), 1);
    assert_eq!(
        engine.check_text("crates/core/src/lib.rs", clean),
        [] as [saispace_rules::Finding; 0]
    );
}

#[test]
fn stop_at_and_suppression_silence_matches() {
    let engine = engine(&[NO_UNWRAP]);
    let code = "\
fn a() {
    let x = y.unwrap(); // sai-ignore: rust-no-unwrap
    // sai-ignore: rust-no-unwrap
    let z = w.expect(\"ok\");
}

#[cfg(test)]
mod tests {
    fn t() { q.unwrap(); }
}
";
    assert_eq!(
        engine.check_text("crates/core/src/lib.rs", code),
        [] as [saispace_rules::Finding; 0]
    );

    // Suppressing a different rule does not silence this one.
    let other = "let x = y.unwrap(); // sai-ignore: rust-no-todo\n";
    assert_eq!(engine.check_text("crates/core/src/lib.rs", other).len(), 1);
}

#[test]
fn guide_only_rule_applies_but_never_reports() {
    let engine = engine(&[GUIDE_ONLY]);
    let rules = engine.rules_for("src/components/Button.tsx");
    assert_eq!(rules.len(), 1);
    assert_eq!(
        rules[0].guide.as_deref(),
        Some("Read DESIGN.md before touching components.")
    );
    assert_eq!(
        engine.check_text("src/components/Button.tsx", "anything goes\n"),
        [] as [saispace_rules::Finding; 0]
    );
}

// -------------------------------------------------------------------- applies

#[test]
fn forbid_reports_every_offending_line_with_position_and_fix() {
    let engine = engine(&[NO_UNWRAP]);
    let code = "fn a() {\n    let v = x.unwrap();\n    let w = y.expect(\"boom\");\n}\n";
    let found = engine.check_text("crates/core/src/lib.rs", code);

    assert_eq!(found.len(), 2);
    assert_eq!((found[0].line, found[0].column), (2, 14));
    assert_eq!(found[0].snippet, "let v = x.unwrap();");
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].message, "No .unwrap() in production code");
    assert_eq!(found[0].fix.as_deref(), Some("Propagate the error with ?"));
    assert_eq!((found[1].line, found[1].column), (3, 14));
}

#[test]
fn forbid_reports_only_the_first_match_on_a_line() {
    let engine = engine(&[NO_UNWRAP]);
    let found = engine.check_text("crates/core/src/lib.rs", "a.unwrap(); b.unwrap();\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].column, 2);
}

#[test]
fn require_reports_once_at_line_one_when_missing() {
    let engine = engine(&[LICENSE]);
    let ok = "// SPDX-License-Identifier: MIT\nexport const a = 1;\n";
    assert_eq!(
        engine.check_text("src/a.ts", ok),
        [] as [saispace_rules::Finding; 0]
    );

    let missing = engine.check_text("src/a.ts", "export const a = 1;\n");
    assert_eq!(missing.len(), 1);
    assert_eq!((missing[0].line, missing[0].column), (1, 1));
    assert_eq!(missing[0].snippet, "export const a = 1;");

    // An empty file also lacks the header.
    assert_eq!(engine.check_text("src/empty.ts", "").len(), 1);
}

#[test]
fn max_lines_reports_the_first_line_over_the_limit() {
    let engine = engine(&[SHORT]);
    assert_eq!(
        engine.check_text("src/components/A.tsx", "1\n2\n3\n"),
        [] as [saispace_rules::Finding; 0]
    );

    let found = engine.check_text("src/components/A.tsx", "1\n2\n3\n4\n5\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 4);
    assert_eq!(found[0].snippet, "5 lines (max 3)");
}

#[test]
fn crlf_line_endings_do_not_leak_into_snippets() {
    let engine = engine(&[NO_UNWRAP]);
    let found = engine.check_text(
        "crates/core/src/lib.rs",
        "fn a() {\r\n    x.unwrap();\r\n}\r\n",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 2);
    assert_eq!(found[0].snippet, "x.unwrap();");
}

// ------------------------------------------------------------------- priority

#[test]
fn overlapping_rules_all_report_and_severity_does_not_shadow() {
    // There is no rule priority: every rule that applies to a file runs and
    // reports independently, even on the same line.
    let engine = engine(&[NO_TODO, NO_UNWRAP]);
    let found = engine.check_text("crates/core/src/lib.rs", "todo!(x.unwrap());\n");

    let mut ids: Vec<(&str, Severity)> = found
        .iter()
        .map(|f| (f.rule.as_str(), f.severity))
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        vec![
            ("rust-no-todo", Severity::Warn),
            ("rust-no-unwrap", Severity::Error)
        ]
    );
    assert!(Severity::Error > Severity::Warn);
}

#[test]
fn tree_report_is_sorted_by_path_line_then_rule_and_counts_severities() {
    let repo = TempRepo::new("order");
    repo.rule("rust-no-todo", NO_TODO);
    repo.rule("rust-no-unwrap", NO_UNWRAP);
    repo.write(
        "crates/b/src/lib.rs",
        "fn b() {\n    todo!(x.unwrap());\n}\n",
    );
    repo.write("crates/a/src/lib.rs", "fn a() { y.unwrap(); }\n");
    repo.write("scripts/tool.rs", "fn t() { todo!(1); z.unwrap(); }\n");

    let rules = load_rules(repo.path()).expect("load");
    assert_eq!(
        rules.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["rust-no-todo", "rust-no-unwrap"]
    );
    let report = Engine::new(&rules)
        .expect("engine")
        .check_tree(repo.path(), &[]);

    let order: Vec<(&str, usize, &str)> = report
        .findings
        .iter()
        .map(|f| (f.path.as_str(), f.line, f.rule.as_str()))
        .collect();
    assert_eq!(
        order,
        vec![
            ("crates/a/src/lib.rs", 1, "rust-no-unwrap"),
            ("crates/b/src/lib.rs", 2, "rust-no-todo"),
            ("crates/b/src/lib.rs", 2, "rust-no-unwrap"),
            ("scripts/tool.rs", 1, "rust-no-todo"),
        ]
    );
    assert_eq!((report.errors(), report.warnings()), (2, 2));
    assert_eq!(report.files_scanned, 3);
    assert_eq!(report.rules, 2);
}

#[test]
fn tree_scan_skips_oversized_and_non_utf8_files_silently() {
    let repo = TempRepo::new("skip");
    repo.write("a.rs", "todo!(1);\n");
    repo.write("latin1.rs", b"// caf\xe9\ntodo!(1);\n");
    let mut big = "todo!(1);\n".to_string();
    big.push_str(&"x".repeat(2 * 1024 * 1024));
    repo.write("big.rs", big);

    let report = engine(&[NO_TODO]).check_tree(repo.path(), &[]);
    let paths: Vec<&str> = report.findings.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["a.rs"]);
    // Known gap: skipped files still count as scanned.
    assert_eq!(report.files_scanned, 3);
}

// -------------------------------------------------------------- invalid input

#[test]
fn malformed_toml_is_rejected_at_parse_time() {
    // Typo in a field name: used to be silently ignored (and `severity`
    // silently defaulted to `error`).
    let typo = r#"
id = "x"
description = "d"
severty = "warn"
files = ["*"]
guide = "g"
"#;
    assert!(parse(typo).is_err());

    let unknown_check_field = r#"
id = "x"
description = "d"
files = ["*"]

[check]
kind = "forbid"
pattern = "a"
stop = "b"
"#;
    assert!(parse(unknown_check_field).is_err());

    let unknown_kind = r#"
id = "x"
description = "d"
files = ["*"]

[check]
kind = "ban"
pattern = "a"
"#;
    assert!(parse(unknown_kind).is_err());

    let bad_severity = r#"
id = "x"
description = "d"
severity = "fatal"
files = ["*"]
guide = "g"
"#;
    assert!(parse(bad_severity).is_err());

    let missing_files = "id = \"x\"\ndescription = \"d\"\nguide = \"g\"\n";
    assert!(parse(missing_files).is_err());

    let negative_max = r#"
id = "x"
description = "d"
files = ["*"]

[check]
kind = "max_lines"
max = -1
"#;
    assert!(parse(negative_max).is_err());
}

#[test]
fn semantically_invalid_rules_are_typed_errors() {
    let cases = [
        (
            "empty pattern",
            "id = \"x\"\ndescription = \"d\"\nfiles = [\"*\"]\n[check]\nkind = \"forbid\"\npattern = \"\"\n",
        ),
        (
            "empty require",
            "id = \"x\"\ndescription = \"d\"\nfiles = [\"*\"]\n[check]\nkind = \"require\"\npattern = \"\"\n",
        ),
        (
            "empty stop_at",
            "id = \"x\"\ndescription = \"d\"\nfiles = [\"*\"]\n[check]\nkind = \"forbid\"\npattern = \"a\"\nstop_at = \"\"\n",
        ),
        (
            "zero max",
            "id = \"x\"\ndescription = \"d\"\nfiles = [\"*\"]\n[check]\nkind = \"max_lines\"\nmax = 0\n",
        ),
        (
            "empty glob",
            "id = \"x\"\ndescription = \"d\"\nfiles = [\" \"]\nguide = \"g\"\n",
        ),
        (
            "blank guide, no check",
            "id = \"x\"\ndescription = \"d\"\nfiles = [\"*\"]\nguide = \"  \"\n",
        ),
        (
            "blank description",
            "id = \"x\"\ndescription = \" \"\nfiles = [\"*\"]\nguide = \"g\"\n",
        ),
        (
            "path-like id",
            "id = \"../x\"\ndescription = \"d\"\nfiles = [\"*\"]\nguide = \"g\"\n",
        ),
    ];
    for (name, toml) in cases {
        let def = parse(toml).unwrap_or_else(|e| panic!("{name}: should parse: {e}"));
        assert!(
            matches!(Engine::new(&[def]), Err(RulesError::Invalid(..))),
            "{name}: should be RulesError::Invalid"
        );
    }

    let bad_stop = parse(
        "id = \"x\"\ndescription = \"d\"\nfiles = [\"*\"]\n[check]\nkind = \"forbid\"\npattern = \"a\"\nstop_at = \"[\"\n",
    )
    .expect("parse");
    let err = Engine::new(&[bad_stop]).err().expect("bad stop_at regex");
    assert!(matches!(err, RulesError::Regex { ref rule, .. } if rule == "x"));
    assert!(err.to_string().starts_with("rule x: invalid regex"));
}

#[test]
fn duplicate_ids_are_rejected_by_the_engine() {
    let a = parse(NO_TODO).expect("parse");
    let err = Engine::new(&[a.clone(), a]).err().expect("duplicate");
    assert!(matches!(err, RulesError::Invalid(ref id, _) if id == "rust-no-todo"));
}

#[test]
fn rule_files_on_disk_are_validated() {
    let repo = TempRepo::new("disk");

    // File name and id disagree: remove/replace by id would miss it.
    repo.rule("old-name", NO_TODO);
    assert!(matches!(
        load_rules(repo.path()),
        Err(RulesError::Invalid(ref id, _)) if id == "rust-no-todo"
    ));
    fs::remove_file(repo.path().join(".saispace/rules/old-name.toml")).expect("cleanup");

    // Invalid TOML points at the offending file.
    repo.rule("broken", "id = \"broken\"\ndescription = ");
    match load_rules(repo.path()) {
        Err(RulesError::Parse { path, .. }) => assert!(path.ends_with("broken.toml")),
        other => panic!("expected Parse error, got {other:?}"),
    }
    fs::remove_file(repo.path().join(".saispace/rules/broken.toml")).expect("cleanup");

    // Non-TOML files in the folder are ignored; a missing folder is empty.
    repo.rule("rust-no-todo", NO_TODO);
    repo.write(".saispace/rules/README.md", "notes");
    assert_eq!(load_rules(repo.path()).expect("load").len(), 1);
    assert_eq!(
        load_rules(&repo.path().join("nowhere")).expect("empty"),
        [] as [saispace_rules::RuleDef; 0]
    );
}

#[test]
fn remove_rule_refuses_ids_that_escape_the_rules_folder() {
    let repo = TempRepo::new("escape");
    repo.write("victim.toml", "keep me");
    let err = remove_rule(repo.path(), "../../victim").expect_err("refused");
    assert!(matches!(err, RulesError::NotFound(_)));
    assert!(repo.path().join("victim.toml").is_file());
}
