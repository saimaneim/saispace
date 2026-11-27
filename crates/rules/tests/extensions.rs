//! Generic rule features used to port a TypeScript rule engine to TOML:
//! file guards, multi-line matches, line and match filters, path references.

use saispace_rules::{Engine, RuleDef, RulesError};
use std::fs;
use std::path::PathBuf;

fn engine(tomls: &[&str]) -> Engine {
    let defs: Vec<RuleDef> = tomls
        .iter()
        .map(|t| toml::from_str(t).expect("valid toml"))
        .collect();
    Engine::new(&defs).expect("valid rules")
}

struct TempRepo(PathBuf);

impl TempRepo {
    fn new() -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("sai-rules-ext-{n}-{}", std::process::id()));
        fs::create_dir_all(dir.join(".git")).expect("create temp repo");
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write fixture");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ASYNC_CLIENT: &str = r#"
id = "no-async-client-component"
description = "Client components cannot be async"
files = ["src/**/*.tsx"]
only_if = '''['"]use client['"]'''

[check]
kind = "forbid"
pattern = 'export\s+(?:default\s+)?async\s+function\s+[A-Z]\w*'
"#;

const SSRF: &str = r#"
id = "fetch-needs-ssrf-guard"
description = "fetch on a variable URL needs a guard"
files = ["src/lib/**/*.ts"]
unless = 'sanitizeUrl|redirect:\s*"manual"'

[check]
kind = "forbid"
pattern = '\bfetch\([^)]*url[^)]*\)'
multiline = true
"#;

#[test]
fn only_if_limits_a_rule_to_files_that_opt_in() {
    let engine = engine(&[ASYNC_CLIENT]);
    let client = "\"use client\";\nexport async function Panel() { return null; }\n";
    let server = "export async function Panel() { return null; }\n";
    let found = engine.check_text("src/panel.tsx", client);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 2);
    assert_eq!(engine.check_text("src/panel.tsx", server).len(), 0);
}

#[test]
fn unless_exempts_files_that_carry_the_guard() {
    let engine = engine(&[SSRF]);
    let unguarded = "export function load(url: string) {\n  return fetch(\n    url,\n  );\n}\n";
    let guarded = format!("import {{ sanitizeUrl }} from './s';\n{unguarded}");
    let found = engine.check_text("src/lib/a.ts", unguarded);
    assert_eq!(found.len(), 1, "match spans lines but is reported once");
    assert_eq!((found[0].line, found[0].column), (2, 10));
    assert_eq!(found[0].snippet, "return fetch(");
    assert_eq!(engine.check_text("src/lib/a.ts", &guarded).len(), 0);
}

#[test]
fn multiline_matches_honour_suppression_and_stop_at() {
    let engine = engine(&[r#"
id = "no-stub-component"
description = "no empty components"
files = ["**/*.tsx"]

[check]
kind = "forbid"
pattern = 'function\s+\w+\(\)\s*\{\s*return\s+null;\s*\}'
multiline = true
stop_at = '^// tests below'
"#]);
    let text = "function A() {\n  return null;\n}\n// sai-ignore: no-stub-component\nfunction B() {\n  return null;\n}\nfunction C() {\n  return null;\n}\n// tests below\nfunction D() { return null; }\n";
    let found = engine.check_text("a.tsx", text);
    let lines: Vec<usize> = found.iter().map(|f| f.line).collect();
    assert_eq!(lines, vec![1, 8]);
}

#[test]
fn empty_anchor_forbids_a_file_even_when_it_is_empty() {
    let engine = engine(&[r#"
id = "no-barrel"
description = "barrel files are forbidden"
files = ["src/**/index.ts"]

[check]
kind = "forbid"
pattern = '\A'
multiline = true
"#]);
    assert_eq!(engine.check_text("src/ui/index.ts", "").len(), 1);
    assert_eq!(
        engine
            .check_text("src/ui/index.ts", "export * from './a';\n")
            .len(),
        1
    );
    assert_eq!(
        engine
            .check_text("src/ui/button.ts", "export const a = 1;")
            .len(),
        0
    );
}

#[test]
fn unless_line_and_except_filter_individual_matches() {
    let engine = engine(&[r#"
id = "no-direct-env"
description = "read env through the env module"
files = ["src/**/*.ts"]

[check]
kind = "forbid"
pattern = 'process\.env(?:\.\w+|\[[^\]]+\])'
except = 'NEXT_PUBLIC_'
unless_line = 'eslint-disable-line'
"#]);
    let text = "const a = process.env.NEXT_PUBLIC_URL;\nconst b = process.env.SECRET;\nconst c = process.env.OTHER; // eslint-disable-line\nconst d = process.env.NEXT_PUBLIC_A + process.env.TOKEN;\n";
    let found = engine.check_text("src/a.ts", text);
    let hits: Vec<(usize, usize)> = found.iter().map(|f| (f.line, f.column)).collect();
    assert_eq!(
        hits,
        vec![(2, 11), (4, 39)],
        "except applies per match, so a later match on the same line is still found"
    );
}

#[test]
fn refs_exist_resolves_paths_against_the_repo_root() {
    let repo = TempRepo::new();
    repo.write("src/lib/env.ts", "export const env = {};\n");
    repo.write("src/ui/button.tsx", "export {};\n");
    repo.write(
        ".agents/checks/cited-paths-exist.toml",
        r#"
id = "cited-paths-exist"
description = "cited path must exist"
files = ["docs/**/*.md"]

[check]
kind = "refs_exist"
pattern = '`(src/[^`]+)`'
suffixes = [".ts", ".tsx"]
"#,
    );
    repo.write(
        "docs/guide.md",
        "Use `src/lib/env` and `src/ui/button`.\nOld: `src/lib/gone`.\n",
    );
    let rules = saispace_rules::load_rules(&repo.0).expect("load");
    let engine = Engine::new(&rules).expect("engine");

    let report = engine.check_tree(&repo.0, &[]);
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!((finding.path.as_str(), finding.line), ("docs/guide.md", 2));
    assert_eq!(
        finding.column, 7,
        "points at the cited path, not the backtick"
    );

    let text = fs::read_to_string(repo.0.join("docs/guide.md")).expect("read");
    assert_eq!(
        engine.check_text("docs/guide.md", &text).len(),
        0,
        "without a root there is nothing to resolve against"
    );
}

#[test]
fn guards_with_empty_or_broken_regex_are_rejected() {
    let base = |extra: &str| {
        format!(
            "id = \"r\"\ndescription = \"d\"\nfiles = [\"*\"]\n{extra}\n[check]\nkind = \"forbid\"\npattern = \"x\"\n"
        )
    };
    let parse = |extra: &str| -> RuleDef { toml::from_str(&base(extra)).expect("toml") };
    assert!(matches!(
        Engine::new(&[parse("unless = \"\"")]),
        Err(RulesError::Invalid(..))
    ));
    assert!(matches!(
        Engine::new(&[parse("only_if = \"(\"")]),
        Err(RulesError::Regex { .. })
    ));
}

#[test]
fn within_scopes_a_pattern_to_the_capture_of_an_enclosing_span() {
    let engine = engine(&[r#"
id = "dialog-title-scale"
description = "DialogTitle keeps the modal type scale"
files = ["src/**/*.tsx"]

[check]
kind = "forbid"
within = '''<DialogTitle\b(?:[^>"']|"[^"]*"|'[^']*')*?className\s*=\s*("[^"]*"|\{[^}]*\})'''
pattern = '''(?:^|[\s"'`{(,])text-[^\s"'`{}(),]*'''
except = '''(?:^|[\s"'`{(,])text-(?:base|lg|left|center|right)$'''
"#]);
    let text = "<Label className=\"text-xs\" />\n<DialogTitle\n  id=\"t\"\n  className=\"text-base text-xs text-center\"\n/>\n<DialogTitle className={cn(\"text-lg\", open && \"text-3xl\")}>Hi</DialogTitle>\n<DialogTitle>plain</DialogTitle>\n";
    let found = engine.check_text("src/a.tsx", text);
    let hits: Vec<(usize, &str)> = found.iter().map(|f| (f.line, f.snippet.as_str())).collect();
    assert_eq!(
        hits,
        vec![
            (4, "className=\"text-base text-xs text-center\""),
            (
                6,
                "<DialogTitle className={cn(\"text-lg\", open && \"text-3xl\")}>Hi</DialogTitle>"
            ),
        ],
        "tokens outside the className span or on other tags are ignored; allowed tokens pass"
    );
    assert_eq!(
        found[0].column, 23,
        "column is absolute in the line, not inside the span"
    );
}

#[test]
fn unless_span_turns_forbid_into_a_stale_opt_in_detector() {
    let engine = engine(&[r#"
id = "stale-gradient-optin"
description = "data-gradient marks nothing; remove it"
files = ["src/**/*.tsx"]

[check]
kind = "forbid"
within = '''<[A-Za-z][\w.]*\b(?:[^>"'{}]|"[^"]*"|'[^']*'|\{[^{}]*\})*>'''
pattern = '\bdata-gradient\s*='
unless_span = 'gradient\('
"#]);
    let text = "<div\n  data-gradient=\"decorative\"\n  className=\"bg-[radial-gradient(at_top,red,blue)]\"\n/>\n<div data-gradient=\"decorative\" className=\"flat\" onClick={() => go()} />\n<span className=\"data-gradient-free\">x</span>\n";
    let found = engine.check_text("src/a.tsx", text);
    let lines: Vec<usize> = found.iter().map(|f| f.line).collect();
    assert_eq!(
        lines,
        vec![5],
        "opted-in tag with a gradient passes; arrow fn in attrs does not end the tag"
    );
    let scoped = engine_with_capture();
    let text = "<div className=\"bg-red-500\" data-opt=\"keep\" />\n<div className=\"bg-red-500\" />\n<div data-opt=\"keep\" title=\"bg-red-500\" />\n";
    let hits: Vec<usize> = scoped
        .check_text("src/a.tsx", text)
        .iter()
        .map(|f| f.line)
        .collect();
    assert_eq!(
        hits,
        vec![2],
        "unless_span sees attributes outside the capture; text outside the capture is not searched"
    );
    let bare = r#"
id = "bad"
description = "d"
files = ["*"]

[check]
kind = "forbid"
pattern = "x"
unless_span = "y"
"#;
    let rule: RuleDef = toml::from_str(bare).expect("toml");
    assert!(matches!(Engine::new(&[rule]), Err(RulesError::Invalid(..))));
}

fn engine_with_capture() -> Engine {
    engine(&[r#"
id = "no-raw-bg"
description = "no bg-red in className unless the tag opts in"
files = ["src/**/*.tsx"]

[check]
kind = "forbid"
within = '''<[A-Za-z]+\b[^>]*?className="([^"]*)"[^>]*>'''
unless_span = 'data-opt'
pattern = 'bg-red-\d+'
"#])
}
