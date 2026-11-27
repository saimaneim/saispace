//! Shared plumbing for the matchers: the file under check, the repository
//! index and small helpers (regex compilation, path templates, locations).

use crate::css::Tokens;
use crate::error::{RulesError, RulesResult};
use crate::markup::{self, Markup};
use regex::{Captures, Regex};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) const SNIPPET_CHARS: usize = 160;

/// Per-rule data computed once per run and shared by all files.
pub(crate) enum Cache {
    Symbols(HashSet<String>),
    Colors(Tokens),
}

/// The repository a run checks: its root, the list of its files (walked on
/// first use unless the caller already has it) and per-rule caches.
pub(crate) struct Repo<'a> {
    root: &'a Path,
    files: OnceLock<Vec<String>>,
    caches: Vec<OnceLock<Cache>>,
}

impl<'a> Repo<'a> {
    pub(crate) fn new(root: &'a Path, rules: usize, files: Option<Vec<String>>) -> Self {
        let cell = OnceLock::new();
        if let Some(files) = files {
            let _ = cell.set(files);
        }
        Self {
            root,
            files: cell,
            caches: (0..rules).map(|_| OnceLock::new()).collect(),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        self.root
    }

    pub(crate) fn files(&self) -> &[String] {
        self.files
            .get_or_init(|| walk(self.root).into_iter().map(|(_, rel)| rel).collect())
    }

    pub(crate) fn cache(&self, rule: usize) -> Option<&OnceLock<Cache>> {
        self.caches.get(rule)
    }
}

/// Every file under `root` (honouring `.gitignore`, skipping `.git`) as
/// absolute path and `/`-separated path relative to `root`.
pub(crate) fn walk(root: &Path) -> Vec<(PathBuf, String)> {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            let path = e.into_path();
            let rel = path
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((path, rel))
        })
        .collect()
}

/// One file under check, with the line index every matcher shares.
pub(crate) struct FileCtx<'a> {
    pub repo: Option<&'a Repo<'a>>,
    pub rel: &'a str,
    pub text: &'a str,
    pub lines: Vec<&'a str>,
    pub starts: Vec<usize>,
    markup: OnceLock<Markup>,
}

impl<'a> FileCtx<'a> {
    pub(crate) fn new(repo: Option<&'a Repo<'a>>, rel: &'a str, text: &'a str) -> Self {
        Self {
            repo,
            rel,
            text,
            lines: text.lines().collect(),
            starts: line_starts(text),
            markup: OnceLock::new(),
        }
    }

    pub(crate) fn line_text(&self, line: usize) -> &str {
        self.lines.get(line - 1).copied().unwrap_or_default()
    }

    pub(crate) fn markup(&self) -> &Markup {
        self.markup.get_or_init(|| markup::parse(self.text))
    }

    pub(crate) fn hit_at_offset(&self, offset: usize) -> Hit {
        let (line, column) = locate(&self.starts, offset);
        Hit::at(self, line, column)
    }
}

/// A violation: 1-based line and column plus the text to show for it.
pub(crate) struct Hit {
    pub line: usize,
    pub column: usize,
    pub snippet: String,
}

impl Hit {
    pub(crate) fn at(ctx: &FileCtx<'_>, line: usize, column: usize) -> Self {
        Self {
            line,
            column,
            // Cut now: a minified line with many hits must not be copied whole each time.
            snippet: snippet(ctx.line_text(line)),
        }
    }

    pub(crate) fn with_snippet(ctx: &FileCtx<'_>, offset: usize, snippet: String) -> Self {
        let (line, column) = locate(&ctx.starts, offset);
        Self {
            line,
            column,
            snippet,
        }
    }
}

/// 1-based line and column of a byte offset, given the byte offset of every line start.
pub(crate) fn locate(line_starts: &[usize], offset: usize) -> (usize, usize) {
    let line = line_starts.partition_point(|&start| start <= offset).max(1);
    (line, offset - line_starts[line - 1] + 1)
}

pub(crate) fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

pub(crate) fn snippet(line: &str) -> String {
    line.trim().chars().take(SNIPPET_CHARS).collect()
}

pub(crate) fn regex(rule: &str, pattern: &str) -> RulesResult<Regex> {
    Regex::new(pattern).map_err(|source| RulesError::Regex {
        rule: rule.to_string(),
        source,
    })
}

pub(crate) fn optional_regex(rule: &str, pattern: Option<&str>) -> RulesResult<Option<Regex>> {
    pattern.map(|p| regex(rule, p)).transpose()
}

/// The scoped part of a match: group 1 when the regex declares one, else the whole match.
pub(crate) fn scope<'h>(re: &Regex, caps: &Captures<'h>) -> Option<regex::Match<'h>> {
    if re.captures_len() > 1 {
        caps.get(1)
    } else {
        caps.get(0)
    }
}

pub(crate) fn is_suppressed(lines: &[&str], index: usize, rule: &str) -> bool {
    let marker = |line: &str| {
        line.split("sai-ignore:")
            .nth(1)
            .is_some_and(|rest| rest.split([',', ' ']).any(|id| id.trim() == rule))
    };
    lines.get(index).is_some_and(|line| marker(line))
        || (index > 0 && lines.get(index - 1).is_some_and(|line| marker(line)))
}

pub(crate) const PATH_PLACEHOLDERS: [&str; 6] = ["path", "dir", "parent", "file", "stem", "ext"];

/// The pieces of a repository-relative path a template can use.
fn path_piece(rel: &str, name: &str) -> Option<String> {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    Some(match name {
        "path" => rel.to_string(),
        "dir" => dir.to_string(),
        "parent" => dir.rsplit('/').next().unwrap_or_default().to_string(),
        "file" => file.to_string(),
        "stem" => stem.to_string(),
        "ext" => file.rsplit_once('.').map_or("", |(_, ext)| ext).to_string(),
        _ => return None,
    })
}

pub(crate) fn has_path_placeholder(text: &str) -> bool {
    PATH_PLACEHOLDERS
        .iter()
        .any(|name| text.contains(&format!("{{{name}}}")))
}

/// Replaces `{0}`, `{1}`.. and `{name}` with captures and the path
/// placeholders with pieces of `rel`; unknown placeholders stay as written.
/// With `escape` the substituted text is regex-escaped.
pub(crate) fn expand(
    template: &str,
    caps: Option<&Captures<'_>>,
    rel: &str,
    escape: bool,
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        let value = name
            .parse::<usize>()
            .ok()
            .and_then(|i| caps.and_then(|c| c.get(i)))
            .map(|m| m.as_str().to_string())
            .or_else(|| {
                caps.and_then(|c| c.name(name))
                    .map(|m| m.as_str().to_string())
            })
            .or_else(|| path_piece(rel, name));
        match value {
            Some(v) if escape => out.push_str(&regex::escape(&v)),
            Some(v) => out.push_str(&v),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}
