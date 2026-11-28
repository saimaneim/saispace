//! Generic engine capabilities: path templates, JSON and glob references,
//! balanced blocks, cross-file symbols and sets, JSX element checks, WCAG
//! contrast and CSS design-token checks. Every input mirrors real project code.

use saispace_rules::{Engine, Finding, RuleDef, RulesError};
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
        let dir = std::env::temp_dir().join(format!("sai-rules-cap-{n}-{}", std::process::id()));
        fs::create_dir_all(dir.join(".git")).expect("create temp repo");
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write fixture");
    }

    fn check(&self, engine: &Engine, rel: &str) -> Vec<Finding> {
        let text = fs::read_to_string(self.0.join(rel)).expect("read fixture");
        engine.check_text_at(Some(&self.0), rel, &text)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn lines(findings: &[Finding]) -> Vec<usize> {
    findings.iter().map(|f| f.line).collect()
}

// ---------------------------------------------------------------- forbid

#[test]
fn unless_before_looks_only_at_the_configured_window_before_a_match() {
    let engine = engine(&[r"
id = 'map-no-array-destructuring'
description = 'name the fields'
files = ['src/**/*.ts']

[check]
kind = 'forbid'
pattern = '\.map\(\s*\(\[[a-z, ]+\]\)'
unless_before = 'Object\.entries'
before_chars = 40
multiline = true
"]);
    let text = "const a = Object.entries(o).map(([k, v]) => k + v);\nconst b = Object.entries(\n  someLongObjectNameThatPushesTheCallFarAway,\n).map(([k, v]) => k + v);\nconst c = list.map(([x]) => x);\n";
    let found = engine.check_text("src/a.ts", text);
    assert_eq!(
        lines(&found),
        vec![4, 5],
        "line 1 is exempt (entries is 0 chars away); line 4 is more than 40 chars after entries"
    );
}

// ---------------------------------------------------------- path templates

#[test]
fn require_with_a_path_placeholder_compares_the_file_with_its_own_location() {
    let engine = engine(&[r"
id = 'skill-name-matches-dir'
description = 'name must match the folder'
files = ['.agents/skills/*/SKILL.md']

[check]
kind = 'require'
pattern = '(?m)^name:\s*{parent}\s*$'
"]);
    let ok = "---\nname: bun\ndescription: x\n---\n";
    let wrong = "---\nname: bunx\ndescription: x\n---\n";
    assert_eq!(
        engine.check_text(".agents/skills/bun/SKILL.md", ok).len(),
        0
    );
    assert_eq!(
        engine
            .check_text(".agents/skills/bun/SKILL.md", wrong)
            .len(),
        1,
        "'bunx' is not 'bun' even though it starts with it"
    );
    assert_eq!(
        engine
            .check_text(".agents/skills/a.b/SKILL.md", "name: aXb\n")
            .len(),
        1,
        "the folder name is escaped, so the dot does not match any character"
    );
}

#[test]
fn refs_exist_templates_turn_captures_and_file_pieces_into_paths() {
    let repo = TempRepo::new();
    repo.write("src/components/button.tsx", "export {};\n");
    repo.write("src/components/button.test.tsx", "export {};\n");
    repo.write("src/components/card.tsx", "export {};\n");
    repo.write(
        "docs/components/button.md",
        "Uses [[Button]] and [[Card]] and [[Modal]].\n",
    );
    let engine = engine(&[r#"
id = "doc-names-a-component"
description = "every [[Name]] needs src/<dir>/<name>.tsx next to its doc"
files = ["docs/*/*.md"]

[check]
kind = "refs_exist"
pattern = '\[\[([A-Za-z]+)\]\]'
template = "src/{parent}/{stem}.tsx"
"#]);
    assert_eq!(
        repo.check(&engine, "docs/components/button.md").len(),
        0,
        "{{parent}} is `components` and {{stem}} is `button` for every reference of this file"
    );

    let by_capture = self::engine(&[r#"
id = "doc-names-a-component"
description = "every [[Name]] needs its source file"
files = ["docs/*/*.md"]

[check]
kind = "refs_exist"
pattern = '\[\[(?P<name>[A-Z][a-z]+)\]\]'
template = "src/{parent}/{name}.stories.tsx"
suffixes = []
"#]);
    let found = repo.check(&by_capture, "docs/components/button.md");
    assert_eq!(
        found.len(),
        3,
        "no Button, Card or Modal stories exist (independent of case-insensitive filesystems)"
    );
    assert_eq!(
        found[2].snippet,
        "Uses [[Button]] and [[Card]] and [[Modal]]."
    );
    assert_eq!(found[0].column, 8, "points at the captured name");
}

#[test]
fn refs_exist_reads_object_keys_from_json() {
    let repo = TempRepo::new();
    repo.write(".agents/skills/bun/SKILL.md", "x\n");
    repo.write(
        "skills-lock.json",
        "{\n  \"version\": 1,\n  \"skills\": {\n    \"bun\": {\"source\": \"a\"},\n    \"ghost\": {\"source\": \"b\"}\n  }\n}\n",
    );
    let engine = engine(&[r#"
id = "skills-lock-synced"
description = "lock entry without skill folder"
files = ["skills-lock.json"]

[check]
kind = "refs_exist"
json_keys = "skills"
template = ".agents/skills/{0}/SKILL.md"
"#]);
    let found = repo.check(&engine, "skills-lock.json");
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].line, found[0].column),
        (5, 6),
        "points at the key `ghost`, not at `bun` or the `skills` object"
    );
}

#[test]
fn refs_exist_glob_must_match_at_least_one_real_file() {
    let repo = TempRepo::new();
    repo.write("src/components/ui/button.tsx", "export {};\n");
    repo.write("src/lib/env.ts", "export {};\n");
    repo.write(
        ".agents/rules/ui.md",
        "---\ntrigger: glob\nglobs: [\"src/components/ui/*.tsx\", \"src/legacy/**/*.ts\", \"src/{lib,app}/*.ts\"]\n---\n",
    );
    let engine = engine(&[r#"
id = "scoped-rule-globs-match-files"
description = "glob matches nothing"
files = [".agents/rules/*.md"]

[check]
kind = "refs_exist"
within = '(?m)^globs:\s*\[([^\]]*)\]'
pattern = '"([^"]+)"'
glob = true
"#]);
    let found = repo.check(&engine, ".agents/rules/ui.md");
    assert_eq!(found.len(), 1, "only src/legacy/**/*.ts has no file");
    assert_eq!(found[0].line, 3);
    assert_eq!(found[0].column, 37);
}

#[test]
fn invalid_reference_configs_are_rejected() {
    let both = toml::from_str::<RuleDef>(
        r"
id = 'x'
description = 'd'
files = ['a']
[check]
kind = 'refs_exist'
pattern = 'a'
json_keys = 'skills'
",
    )
    .expect("parses");
    assert!(matches!(Engine::new(&[both]), Err(RulesError::Invalid(..))));
    let neither = toml::from_str::<RuleDef>(
        r"
id = 'x'
description = 'd'
files = ['a']
[check]
kind = 'refs_exist'
",
    )
    .expect("parses");
    assert!(matches!(
        Engine::new(&[neither]),
        Err(RulesError::Invalid(..))
    ));
}

// ------------------------------------------------------------------ block

const TRIGGER: &str = r#"
id = "overlay-trigger-cursor-pointer"
description = "overlay triggers need cursor-pointer"
files = ["src/components/ui/*.tsx"]

[check]
kind = "block"
open = 'function\s+\w*Trigger\s*\([\s\S]*?\)\s*(?::[^{]*)?\{'
when = '\wPrimitive\.\w*Trigger\b'
require = 'cursor-pointer'
"#;

#[test]
fn block_with_balanced_braces_ends_at_the_matching_brace_not_the_first_one() {
    let engine = engine(&[TRIGGER]);
    let text = r#"function MenuTrigger({ className, ...props }: Props) {
  const style = { width: "}" };
  if (props.open) {
    return null;
  }
  return <MenuPrimitive.Trigger className={cn("a", className)} {...props} />;
}

function GoodTrigger({ className }: Props) {
  return <MenuPrimitive.Trigger className="cursor-pointer" />;
}

function PlainTrigger() {
  return <div>cursor-pointer lives outside this block</div>;
}

const marker = "cursor-pointer";
"#;
    let found = engine.check_text("src/components/ui/menu.tsx", text);
    assert_eq!(
        lines(&found),
        vec![1],
        "MenuTrigger lacks it; GoodTrigger has it; PlainTrigger is not an overlay; the string after the blocks is outside"
    );
}

#[test]
fn block_end_regex_and_forbid_inside_blocks() {
    let engine = engine(&[r#"
id = "no-console-in-handlers"
description = "no console in handlers"
files = ["src/**/*.ts"]

[check]
kind = "block"
open = '^export function handle\w+\([^)]*\) \{'
end = '\n\}'
forbid = 'console\.'
"#]);
    let text = "export function handleA(x) {\n  if (x) {\n    console.log(x);\n  }\n}\nconsole.log('top level is fine');\nexport function handleB(x) {\n  return x;\n}\n";
    let found = engine.check_text("src/a.ts", text);
    assert_eq!(lines(&found), vec![3]);
}

// ---------------------------------------------------------- unused symbols

const UNUSED: &str = r#"
id = "css-token-unused"
description = "token declared and never consumed"
files = ["src/app/globals.css"]

[check]
kind = "unused_symbols"
declare = '(?m)^\s{2}(--[\w-]+):'
within = '(?s):root \{(.*?)@theme'
corpus = ["src/**/*.tsx"]
uses = [
  { pattern = 'var\(\s*(--[\w-]+)' },
  { pattern = 'var\(\s*(--[\w-]+)', own_file = true },
  { pattern = '\b(?:bg|text|border)-([a-z][\w-]*)', symbol = "--{1}" },
]
"#;

#[test]
fn unused_symbols_are_found_across_files_and_through_utility_names() {
    let repo = TempRepo::new();
    repo.write(
        "src/app/globals.css",
        ":root {\n  --background: #000;\n  --muted: #111;\n  --accent: #222;\n  --dead: #333;\n  --layout-gap: 4px;\n}\n@theme {\n  --color-background: var(--background);\n  --dead-in-theme: 1px;\n}\n.a { gap: var(--layout-gap); }\n",
    );
    repo.write(
        "src/app/page.tsx",
        "export const A = <div className=\"bg-background text-muted\" />;\n",
    );
    repo.write(
        "src/lib/other.tsx",
        "export const B = <i style={{ color: 'var(--accent)' }} />;\n",
    );
    repo.write("src/lib/other.ts", "const unused = 'var(--dead)';\n");
    let engine = engine(&[UNUSED]);
    let found = repo.check(&engine, "src/app/globals.css");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].line, found[0].column),
        (5, 3),
        "--dead is only named in a .ts file outside the corpus; --layout-gap is used by this file's own var()"
    );
    assert_eq!(found[0].snippet, "--dead: #333;");
}

// ------------------------------------------------------------ set covered

#[test]
fn set_covered_reports_each_missing_value_once_and_honours_ignore_and_fallback_files() {
    let repo = TempRepo::new();
    repo.write(
        "src/scraper.ts",
        "const a = { platform: \"twitter\" };\nconst b = { platform: 'github' };\nconst c = { platform: \"bluesky\" };\nconst d = { platform: \"bluesky\" };\nconst e = { platform: \"instagram\" };\n",
    );
    repo.write(
        "src/dialog.tsx",
        "const PLATFORMS = [{ id: \"twitter\" }, { id: \"instagram\" }];\n",
    );
    let engine = engine(&[r#"
id = "scraper-dialog-parity"
description = "the dialog must list every scraped network"
files = ["src/scraper.ts"]

[check]
kind = "set_covered"
pattern = '''platform:\s*["']([a-z0-9_-]+)["']'''
in_files = ["src/helpers.tsx", "src/dialog.tsx"]
in_pattern = '''id:\s*["']([a-z0-9_-]+)["']'''
ignore = ["github", "discord"]
"#]);
    let found = repo.check(&engine, "src/scraper.ts");
    assert_eq!(
        lines(&found),
        vec![3],
        "helpers.tsx does not exist so dialog.tsx is used; github is ignored; bluesky is reported once"
    );
}

// ----------------------------------------------------------------- element

fn element_engine(body: &str) -> Engine {
    engine(&[&format!(
        "id = 'el'\ndescription = 'el'\nfiles = ['src/**/*.tsx']\n[check]\nkind = 'element'\n{body}"
    )])
}

#[test]
fn element_content_and_ancestor_select_dynamic_text_inside_cards() {
    let engine = element_engine(
        r#"
tag = 'p|span'
content = '^\s*\{\s*([A-Za-z_$][\w$.]*)\s*\}\s*$'
content_except = '^(?:count|children)$'
open_not = ['className="[^"]*truncate']
[check.ancestor]
open = 'rounded-xl'
"#,
    );
    let text = r#"export function Card({ item, count }) {
  return (
    <div className="rounded-xl border">
      <span>{item.name}</span>
      <span className="truncate">{item.name}</span>
      <p>{count}</p>
      <p>Static {item.name}</p>
      <section>
        <p>{item.title}</p>
      </section>
    </div>
  );
}
export function Plain({ item }) {
  return <div><p>{item.name}</p></div>;
}
"#;
    let found = engine.check_text("src/a.tsx", text);
    assert_eq!(
        lines(&found),
        vec![4, 9],
        "a bare dynamic child inside the card, nested or not; truncated, counted, mixed or outside-card text is fine"
    );
}

#[test]
fn element_ancestor_window_ignores_cards_that_are_too_far_away() {
    let engine = element_engine(
        r"
tag = 'span'
[check.ancestor]
open = 'rounded-xl'
within_chars = 60
",
    );
    let near = "<div className=\"rounded-xl\"><span>a</span></div>\n";
    let far = format!(
        "<div className=\"rounded-xl\">{}<span>a</span></div>\n",
        " ".repeat(100)
    );
    assert_eq!(engine.check_text("src/a.tsx", near).len(), 1);
    assert_eq!(engine.check_text("src/a.tsx", &far).len(), 0);
}

#[test]
fn element_compare_with_the_parent_resolves_consts_and_render_props() {
    let engine = element_engine(
        r#"
tag = 'HugeiconsIcon'
resolve_consts = true
[check.compare]
to = 'parent'
own = '\bsize-([\d.]+)'
single = true
related = '(?s)(?:\A<Button\b|render=\{\s*<Button\b).*?\bsize="([a-z-]+)"'
[check.compare.allow]
icon-sm = ['3.5']
lg = ['4']
"#,
    );
    let text = r#"const small = "size-3.5";
const wrong = "size-5";
export function Icons() {
  return (
    <div>
      <Button size="icon-sm"><HugeiconsIcon icon={A} className={small} /></Button>
      <Button size="icon-sm"><HugeiconsIcon icon={A} className={wrong} /></Button>
      <Button size="lg">
        <HugeiconsIcon icon={A} className="size-4" />
      </Button>
      <Button size="lg">
        <HugeiconsIcon icon={A} className="size-4 size-5" />
      </Button>
      <Menu render={<Button size="lg" />}>
        <HugeiconsIcon icon={A} className="size-3" />
      </Menu>
      <Card><HugeiconsIcon icon={A} className="size-3" /></Card>
    </div>
  );
}
"#;
    let found = engine.check_text("src/a.tsx", text);
    assert_eq!(
        lines(&found),
        vec![7, 15],
        "wrong const (size-5 in icon-sm slot) and size-3 in an lg slot; two sizes are skipped by `single`; Card has no size"
    );
}

#[test]
fn element_distinct_counts_values_after_resolving_a_class_variable() {
    let engine = element_engine(
        r"
tag = 'HugeiconsIcon'
resolve_consts = true
[check.distinct]
pattern = '\bsize-([\d.]+)'
max = 1
",
    );
    let text = "const twice = cn(\"size-3 size-5\", \"size-3\");\nexport const A = <HugeiconsIcon className={twice} />;\nexport const B = <HugeiconsIcon className=\"size-4 sm:size-4\" />;\nexport const C = <HugeiconsIcon className=\"size-4 sm:size-5\" />;\n";
    let found = engine.check_text("src/a.tsx", text);
    assert_eq!(lines(&found), vec![2, 4]);
}

#[test]
fn element_child_values_and_sibling_comparison() {
    let footer = element_engine(
        r#"
tag = 'DialogFooter'
[check.child_values]
tag = 'Button'
pattern = 'size="([\w-]+)"'
default = 'lg'
max = 1
"#,
    );
    let text = "<DialogFooter>\n  <Button size=\"sm\">a</Button>\n  <Button>b</Button>\n</DialogFooter>\n<DialogFooter><Button>a</Button><Button size=\"lg\">b</Button></DialogFooter>\n<DialogFooter><Button size=\"sm\">a</Button>{ok && <Button size=\"md\">b</Button>}</DialogFooter>\n";
    assert_eq!(
        lines(&footer.check_text("src/a.tsx", text)),
        vec![1, 6],
        "no size means lg, so line 5 is consistent; a button inside {{}} is still a child"
    );

    let row = element_engine(
        r#"
tag = 'Button'
open_not = ['\babsolute\b']
[check.ancestor]
nearest = true
open = '\bflex\b'
open_not = '\bflex-col\b'
[check.compare]
to = 'sibling'
related_tag = 'Input'
own = 'size="([\w-]+)"'
own_default = 'lg'
related = 'size="([\w-]+)"'
related_default = 'md'
[check.compare.deny]
md = ['sm', 'icon']
sm = ['xs']
"#,
    );
    let text = r#"<div className="flex gap-2">
  <Input />
  <Button size="sm">a</Button>
  <Button size="lg">b</Button>
  <Button size="icon" className="absolute">c</Button>
</div>
<div className="flex flex-col">
  <Input />
  <Button size="sm">d</Button>
</div>
<div className="flex"><Input size="sm" /><Button size="xs">e</Button><Button size="sm">f</Button></div>
"#;
    assert_eq!(
        lines(&row.check_text("src/a.tsx", text)),
        vec![3, 11],
        "default Input is md so sm is too small; lg is fine; absolute buttons and flex-col rows are skipped; xs next to Input sm"
    );
}

#[test]
fn element_class_conditional_and_class_strings_follow_how_the_expression_is_written() {
    let engine = element_engine(
        r"
open_not = ['\bdata-border\b']
class_conditional = false
scope = 'class_strings'
pattern = '(?:^|\s)border-white/(?:[0-5]|2[1-9])(?:\s|$)'
",
    );
    let text = r#"export function A({ on }) {
  return (
    <div>
      <div className="border-white/3" />
      <div className={on ? "border-white/3" : "border-white/10"} />
      <div className={cn("border-white/3", on && "x")} />
      <div className={cn("border-white/3", "p-2")} />
      <div className={`border-white/25 ${on ? "border-white/40" : "p-1"}`} />
      <div data-border className="border-white/3" />
      <div
        className={`p-2
          border-white/1`}
      />
    </div>
  );
}
"#;
    let found = engine.check_text("src/a.tsx", text);
    assert_eq!(
        lines(&found),
        vec![4, 7, 8, 10],
        "ternary and && make the class conditional and are exempt; a template is not conditional but only its literal text counts (the ${{}} branch is ignored); hits sit on the element's line"
    );
}

#[test]
fn element_pattern_reports_each_match_on_its_own_line_inside_a_multiline_tag() {
    let engine = element_engine(
        r"
tag = 'HugeiconsIcon'
pattern = '\bsize-[\d.]+'
except = '^size-(?:3|4)$'
",
    );
    let text =
        "<HugeiconsIcon\n  icon={A}\n  className=\"size-4 size-6\"\n  data-x=\"size-9\"\n/>\n";
    let found = engine.check_text("src/a.tsx", text);
    assert_eq!(
        found.iter().map(|f| (f.line, f.column)).collect::<Vec<_>>(),
        vec![(3, 21), (4, 11)]
    );
}

#[test]
fn element_parser_ignores_generics_comparisons_and_strings_that_look_like_tags() {
    let engine = element_engine("tag = 'div'\n");
    let text = "const a = useState<Map<string, number>>(new Map());\nconst b = x < y && y > z;\nconst s = \"<div>\" + '</div>' + `<div>${1 < 2}</div>`;\n// <div> in a comment\n/* <div></div> */\nexport const C = <div>{\"}\"}</div>;\nexport const D = cond ? <div /> : null;\n";
    let found = engine.check_text("src/a.tsx", text);
    assert_eq!(lines(&found), vec![6, 7]);
}

// ---------------------------------------------------------------- contrast

const PALETTE: &str = ":root {\n  --color-white: #ffffff;\n  --color-black: #000000;\n  --color-transparent: transparent;\n  --color-gray-400: #9ca3af;\n  --color-yellow-300: #fde047;\n}\n";
const THEME: &str = ":root {\n  --background: hsl(0 0% 9%);\n  --foreground: hsl(0 0% 95%);\n  --primary: hsl(0 0% 96%);\n  --primary-foreground: hsl(0 0% 9%);\n  --muted-foreground: hsl(0 0% 64%);\n}\n@theme {\n  --color-background: var(--background);\n  --color-foreground: var(--foreground);\n  --color-primary: var(--primary);\n  --color-primary-foreground: var(--primary-foreground);\n  --color-muted-foreground: var(--muted-foreground);\n}\n";

fn contrast_repo() -> TempRepo {
    let repo = TempRepo::new();
    repo.write("src/app/globals.css", THEME);
    repo.write("palette.css", PALETTE);
    repo
}

const CONTRAST_JSX: &str = r##"
id = "contrast-jsx"
description = "contrast"
files = ["src/**/*.tsx"]

[check]
kind = "contrast"
units = "jsx"
colors = ["src/app/globals.css", "palette.css"]
default_fg = "#eaedf0"
default_bg = "#0b0c0f"
large = '^text-(?:[2-9]?xl)$'
large_size = '^text-lg$'
bold = '^font-(?:bold|semibold)$'
skip_open = '''\baria-hidden(?:[\s/>]|=\{)|\bsr-only\b'''

[check.sibling_background]
when_fg = '^text-primary-foreground$'
sibling = '^bg-white$'
apply = "bg-white"
"##;

#[test]
fn contrast_jsx_cascades_ancestor_classes_and_reports_the_worst_alternative() {
    let repo = contrast_repo();
    repo.write(
        "src/a.tsx",
        r#"export function A({ on }) {
  return (
    <div className="bg-white">
      <p className="text-white">white on white</p>
      <p className="text-black">black on white</p>
      <p className="text-gray-400">gray</p>
      <p className="text-yellow-300 text-xl">large yellow</p>
      <p className="text-lg font-bold text-gray-400">bold lg gray</p>
      <p className={on ? "text-black" : "text-white"}>either</p>
      <p aria-hidden className="text-white">hidden</p>
      <p className="text-white sr-only">only for readers</p>
      <p className="text-white bg-black/90">alpha bg over white</p>
      <ul>
        {items.map((i) => <li className="text-white" key={i} />)}
      </ul>
    </div>
  );
}
"#,
    );
    let engine = engine(&[CONTRAST_JSX]);
    let found = repo.check(&engine, "src/a.tsx");
    assert_eq!(
        lines(&found),
        vec![4, 6, 7, 8, 9],
        "white on white; gray-400 (2.5:1); yellow text-xl on white (1.4:1 < 3); bold lg gray 2.5:1 < 3; the white branch of the ternary. Hidden, sr-only, alpha-on-dark and the empty `.map` container pass"
    );
    assert!(
        found[0].snippet.starts_with("<p> 1.00:1 (min 4.5:1)"),
        "{}",
        found[0].snippet
    );
    assert!(
        found[1]
            .snippet
            .contains("text-gray-400 (#9ca3af) on bg bg-white (#ffffff)"),
        "{}",
        found[1].snippet
    );
    assert!(
        found[2].snippet.contains("(min 3:1)"),
        "large text needs 3:1"
    );
}

#[test]
fn contrast_jsx_checks_hover_state_and_sibling_indicator_backgrounds() {
    let repo = contrast_repo();
    repo.write(
        "src/b.tsx",
        r#"export function B() {
  return (
    <nav>
      <span className="absolute bg-white" />
      <button className="text-primary-foreground">on the pill</button>
      <button className="bg-primary text-primary-foreground hover:bg-white hover:text-white">hover</button>
      <button className="bg-primary text-primary-foreground hover:text-primary-foreground">fine</button>
      <label className="text-primary-foreground bg-primary">bg wins over the pill</label>
    </nav>
  );
}
"#,
    );
    let engine = engine(&[CONTRAST_JSX]);
    let found = repo.check(&engine, "src/b.tsx");
    assert_eq!(
        lines(&found),
        vec![6],
        "line 5 text-primary-foreground (#171717) sits on the white pill sibling; line 6's hover:white on hover:white is 1:1"
    );
    assert!(found[0].snippet.contains("[hover]"), "{}", found[0].snippet);
}

#[test]
fn contrast_variants_checks_every_variant_state_and_icon_color() {
    let repo = contrast_repo();
    repo.write(
        "src/ui/button.tsx",
        r#"const buttonVariants = cva("inline-flex text-xs", {
  variants: {
    variant: {
      ok: "bg-primary text-primary-foreground hover:bg-primary/90",
      bad: "bg-white text-gray-400",
      icon: "bg-white text-black [&_svg]:text-white",
      hover: "bg-primary text-primary-foreground hover:bg-white hover:text-white",
      active: ["bg-primary", "text-primary-foreground", "active:bg-white active:text-white"],
    },
    size: { sm: "bg-white text-white" },
  },
});
const plain = cva("bg-white text-white");
"#,
    );
    let engine = engine(&[r##"
id = "contrast-cva"
description = "contrast"
files = ["src/ui/*.tsx"]

[check]
kind = "contrast"
units = "variants"
colors = ["src/app/globals.css", "palette.css"]
default_fg = "#eaedf0"
default_bg = "#0b0c0f"
min_emphasis = 3.5
emphasis = '^font-(?:medium|semibold)$'
"##]);
    let found = repo.check(&engine, "src/ui/button.tsx");
    let labels: Vec<&str> = found
        .iter()
        .map(|f| f.snippet.split(" 1.").next().unwrap_or(&f.snippet))
        .collect();
    assert_eq!(found.len(), 5, "{labels:?}");
    assert!(
        found[0].snippet.starts_with("<buttonVariants.bad> 2.5"),
        "{}",
        found[0].snippet
    );
    assert!(
        found[1]
            .snippet
            .starts_with("<buttonVariants.icon> [icon] 1.00:1"),
        "{}",
        found[1].snippet
    );
    assert!(
        found[2]
            .snippet
            .starts_with("<buttonVariants.hover> [hover] 1.00:1")
    );
    assert!(
        found[3]
            .snippet
            .starts_with("<buttonVariants.active> [active] 1.00:1")
    );
    assert!(found[4].snippet.starts_with("<plain> 1.00:1"));
    assert_eq!(
        found.iter().map(|f| f.line).collect::<Vec<_>>(),
        vec![1, 1, 1, 1, 13],
        "findings sit on the cva( call; the `size` group is not evaluated"
    );
}

#[test]
fn contrast_without_a_color_source_never_reports() {
    let repo = TempRepo::new();
    repo.write(
        "src/a.tsx",
        "export const A = <p className=\"text-white bg-white\">x</p>;\n",
    );
    let engine = engine(&[CONTRAST_JSX]);
    assert_eq!(
        repo.check(&engine, "src/a.tsx").len(),
        0,
        "unknown color names are not guessed"
    );
}

// -------------------------------------------------------------- css tokens

fn css_engine(body: &str) -> Engine {
    engine(&[&format!(
        "id = 'css'\ndescription = 'css'\nfiles = ['**/*.css']\n[check]\n{body}"
    )])
}

#[test]
fn css_length_resolves_rem_calc_and_var_chains() {
    let engine = css_engine("kind = 'css_length'\nnames = '^text-[a-z0-9]+$'\nmin_px = 13.0\n");
    let css = "@theme {\n  --text-xs: 0.75rem;\n  --text-sm: calc(var(--base) - 1px);\n  --text-base: var(--base);\n  --text-xs--line-height: 0.5;\n  --text-mono: 12px; /* comment: --text-fake: 1px; */\n}\n:root { --base: 1rem; }\n";
    let found = engine.check_text("a.css", css);
    assert_eq!(lines(&found), vec![2, 6]);
    assert_eq!(found[0].snippet, "--text-xs: 0.75rem = 12px");
}

#[test]
fn css_line_height_uses_the_ratio_of_unitless_and_calc_values() {
    let engine = css_engine(
        "kind = 'css_line_height'\nnames = '^text-[a-z0-9]+$'\nsuffix = '--line-height'\nmin_ratio = 1.05\nskip_from_px = 64.0\n",
    );
    let css = "@theme {\n  --text-xs: 0.8125rem;\n  --text-xs--line-height: calc(1.3 / 0.8125);\n  --text-sm: 0.9375rem;\n  --text-sm--line-height: 0.9;\n  --text-7xl: 4.5rem;\n  --text-7xl--line-height: 1;\n  --text-9xl: 8rem;\n  --text-9xl--line-height: 0.9;\n}\n";
    let found = engine.check_text("a.css", css);
    assert_eq!(
        lines(&found),
        vec![5],
        "calc is 1.6x; a 72px display at 1x is exempt; 128px too"
    );
    assert!(found[0].snippet.contains("0.900x of 15px"));
}

#[test]
fn css_scale_flags_duplicates_and_steps_that_cannot_be_seen() {
    let engine =
        css_engine("kind = 'css_scale'\nnames = '^radius-[a-z0-9]+$'\nmin_step_px = 2.0\n");
    let css = ":root { --radius: 10px; }\n@theme {\n  --radius-sm: calc(var(--radius) - 4px);\n  --radius-md: calc(var(--radius) - 4px);\n  --radius-lg: calc(var(--radius) - 3px);\n  --radius-xl: calc(var(--radius) + 4px);\n  --radius-odd: calc(var(--missing) + 4px);\n}\n";
    let found = engine.check_text("a.css", css);
    let messages: Vec<&str> = found.iter().map(|f| f.snippet.as_str()).collect();
    assert_eq!(found.len(), 3, "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("cannot resolve")));
    assert!(
        messages
            .iter()
            .any(|m| m.contains("--radius-md") && m.contains("both 6px"))
    );
    assert!(messages.iter().any(|m| m.contains("differ by 1px")));
}

#[test]
fn css_contrast_follows_var_chains_and_blends_alpha_over_the_background() {
    let engine = css_engine(
        r"kind = 'css_contrast'
pairs = [
  { fg = 'color-foreground', bg = 'color-background', min = 4.5, label = 'text' },
  { fg = 'color-border', bg = 'color-background', min = 1.5, label = 'border' },
  { fg = 'color-ghost', bg = 'color-background', min = 3.0, label = 'ghost' },
]
",
    );
    let css = ":root {\n  --background: hsl(0 0% 9%);\n  --foreground: hsl(0 0% 95%);\n  --border: hsl(0 0% 100% / 0.04);\n}\n@theme {\n  --color-background: var(--background);\n  --color-foreground: var(--foreground);\n  --color-border: var(--border);\n}\n";
    let found = engine.check_text("a.css", css);
    let messages: Vec<&str> = found.iter().map(|f| f.snippet.as_str()).collect();
    assert_eq!(found.len(), 2, "{messages:?}");
    assert!(messages[0].starts_with("border: 1.") && messages[0].ends_with("(min 1.5:1)"));
    assert!(messages[1].starts_with("ghost: cannot read"));
}

#[test]
fn css_refs_requires_every_var_to_be_declared() {
    let engine = css_engine("kind = 'css_refs'\nnames = '^(?:radius|color)-'\n");
    let css = ":root { --radius: 4px; --card: red; }\n@theme {\n  --radius-sm: calc(var(--radius) - 2px);\n  --color-card: var(--card);\n  --color-lost: var(--lost, red);\n  --font-x: var(--font-external);\n}\n";
    let found = engine.check_text("a.css", css);
    assert_eq!(lines(&found), vec![5], "--font-x is outside `names`");
    assert!(found[0].snippet.contains("var(--lost)"));
}
