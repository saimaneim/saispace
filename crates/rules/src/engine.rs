use crate::error::{RulesError, RulesResult};
use crate::model::{Check, RuleDef, Severity};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const SNIPPET_CHARS: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub files_scanned: usize,
    pub rules: usize,
    pub findings: Vec<Finding>,
}

impl Report {
    #[must_use]
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    #[must_use]
    pub fn warnings(&self) -> usize {
        self.findings.len() - self.errors()
    }
}

struct ForbidSpec {
    pattern: Regex,
    stop_at: Option<Regex>,
    multiline: bool,
    unless_line: Option<Regex>,
    except: Option<Regex>,
    within: Option<Regex>,
    unless_span: Option<Regex>,
}

enum Matcher {
    Forbid(ForbidSpec),
    RefsExist {
        pattern: Regex,
        suffixes: Vec<String>,
    },
    Require {
        pattern: Regex,
    },
    MaxLines {
        max: usize,
    },
    None,
}

/// One file under check, with the line index every matcher shares.
struct FileCtx<'a> {
    root: Option<&'a Path>,
    text: &'a str,
    lines: Vec<&'a str>,
    starts: Vec<usize>,
}

impl FileCtx<'_> {
    fn line_text(&self, line: usize) -> &str {
        self.lines.get(line - 1).copied().unwrap_or_default()
    }
}

/// A violation: 1-based line and column plus the text to show for it.
struct Hit {
    line: usize,
    column: usize,
    snippet: String,
}

impl Hit {
    fn at(ctx: &FileCtx<'_>, line: usize, column: usize) -> Self {
        Self {
            line,
            column,
            snippet: ctx.line_text(line).to_string(),
        }
    }
}

struct Compiled {
    def: RuleDef,
    include: GlobSet,
    exclude: GlobSet,
    only_if: Option<Regex>,
    unless: Option<Regex>,
    matcher: Matcher,
}

impl Compiled {
    fn applies(&self, rel: &str) -> bool {
        self.include.is_match(rel) && !self.exclude.is_match(rel)
    }

    fn passes_file_guards(&self, text: &str) -> bool {
        self.only_if.as_ref().is_none_or(|re| re.is_match(text))
            && !self.unless.as_ref().is_some_and(|re| re.is_match(text))
    }
}

impl ForbidSpec {
    fn hits(&self, ctx: &FileCtx<'_>, rule: &str) -> Vec<Hit> {
        let skipped = |m: &regex::Match, line: usize| {
            self.except
                .as_ref()
                .is_some_and(|re| re.is_match(m.as_str()))
                || self
                    .unless_line
                    .as_ref()
                    .is_some_and(|re| re.is_match(ctx.line_text(line)))
                || is_suppressed(&ctx.lines, line - 1, rule)
        };
        let stop_line = self.stop_at.as_ref().map_or(usize::MAX, |stop| {
            ctx.lines
                .iter()
                .position(|l| stop.is_match(l))
                .unwrap_or(usize::MAX)
        });
        let mut hits = Vec::new();
        if let Some(within) = &self.within {
            for caps in within.captures_iter(ctx.text) {
                let (Some(whole), Some(span)) = (caps.get(0), caps.get(1).or_else(|| caps.get(0)))
                else {
                    continue;
                };
                if self
                    .unless_span
                    .as_ref()
                    .is_some_and(|re| re.is_match(whole.as_str()))
                {
                    continue;
                }
                for m in self.pattern.find_iter(span.as_str()) {
                    let (line, column) = locate(&ctx.starts, span.start() + m.start());
                    if line > stop_line {
                        break;
                    }
                    if !skipped(&m, line) {
                        hits.push(Hit::at(ctx, line, column));
                    }
                }
            }
        } else if self.multiline {
            for m in self.pattern.find_iter(ctx.text) {
                let (line, column) = locate(&ctx.starts, m.start());
                if line > stop_line {
                    break;
                }
                if !skipped(&m, line) {
                    hits.push(Hit::at(ctx, line, column));
                }
            }
        } else {
            for (index, line) in ctx.lines.iter().enumerate().take(stop_line) {
                let found = self
                    .pattern
                    .find_iter(line)
                    .find(|m| !skipped(m, index + 1));
                if let Some(m) = found {
                    hits.push(Hit::at(ctx, index + 1, m.start() + 1));
                }
            }
        }
        hits
    }
}

impl Compiled {
    fn hits(&self, ctx: &FileCtx<'_>) -> Vec<Hit> {
        match &self.matcher {
            Matcher::Forbid(spec) => spec.hits(ctx, &self.def.id),
            Matcher::RefsExist { pattern, suffixes } => {
                let Some(root) = ctx.root else {
                    return Vec::new();
                };
                pattern
                    .captures_iter(ctx.text)
                    .filter_map(|caps| {
                        let reference = caps.get(1).or_else(|| caps.get(0))?;
                        let (line, column) = locate(&ctx.starts, reference.start());
                        let missing = !path_exists(root, reference.as_str(), suffixes)
                            && !is_suppressed(&ctx.lines, line - 1, &self.def.id);
                        missing.then(|| Hit::at(ctx, line, column))
                    })
                    .collect()
            }
            Matcher::Require { pattern } => {
                if pattern.is_match(ctx.text) {
                    Vec::new()
                } else {
                    vec![Hit::at(ctx, 1, 1)]
                }
            }
            Matcher::MaxLines { max } if ctx.lines.len() > *max => vec![Hit {
                line: *max + 1,
                column: 1,
                snippet: format!("{} lines (max {max})", ctx.lines.len()),
            }],
            Matcher::MaxLines { .. } | Matcher::None => Vec::new(),
        }
    }
}

pub struct Engine {
    rules: Vec<Compiled>,
}

fn glob_set(rule: &str, globs: &[String]) -> RulesResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for glob in globs {
        let compiled = Glob::new(glob).map_err(|source| RulesError::Glob {
            rule: rule.to_string(),
            source,
        })?;
        builder.add(compiled);
    }
    builder.build().map_err(|source| RulesError::Glob {
        rule: rule.to_string(),
        source,
    })
}

fn regex(rule: &str, pattern: &str) -> RulesResult<Regex> {
    Regex::new(pattern).map_err(|source| RulesError::Regex {
        rule: rule.to_string(),
        source,
    })
}

/// 1-based line and column of a byte offset, given the byte offset of every line start.
fn locate(line_starts: &[usize], offset: usize) -> (usize, usize) {
    let line = line_starts.partition_point(|&start| start <= offset).max(1);
    (line, offset - line_starts[line - 1] + 1)
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

fn path_exists(root: &Path, reference: &str, suffixes: &[String]) -> bool {
    std::iter::once(String::new())
        .chain(suffixes.iter().cloned())
        .any(|suffix| root.join(format!("{reference}{suffix}")).exists())
}

fn optional_regex(rule: &str, pattern: Option<&str>) -> RulesResult<Option<Regex>> {
    pattern.map(|p| regex(rule, p)).transpose()
}

fn is_suppressed(lines: &[&str], index: usize, rule: &str) -> bool {
    let marker = |line: &str| {
        line.split("sai-ignore:")
            .nth(1)
            .is_some_and(|rest| rest.split([',', ' ']).any(|id| id.trim() == rule))
    };
    lines.get(index).is_some_and(|line| marker(line))
        || (index > 0 && lines.get(index - 1).is_some_and(|line| marker(line)))
}

fn snippet(line: &str) -> String {
    line.trim().chars().take(SNIPPET_CHARS).collect()
}

impl Engine {
    /// # Errors
    ///
    /// Returns an error if a rule is invalid, two rules share an id, or one of
    /// its patterns cannot be compiled.
    pub fn new(defs: &[RuleDef]) -> RulesResult<Self> {
        let mut seen = std::collections::HashSet::new();
        if let Some(dup) = defs.iter().find(|d| !seen.insert(d.id.as_str())) {
            return Err(RulesError::Invalid(
                dup.id.clone(),
                "id is used by more than one rule".to_string(),
            ));
        }
        let rules = defs
            .iter()
            .map(|def| {
                def.validate()?;
                let matcher = match &def.check {
                    Some(Check::Forbid {
                        pattern,
                        stop_at,
                        multiline,
                        unless_line,
                        except,
                        within,
                        unless_span,
                    }) => Matcher::Forbid(ForbidSpec {
                        pattern: regex(&def.id, pattern)?,
                        stop_at: optional_regex(&def.id, stop_at.as_deref())?,
                        multiline: *multiline,
                        unless_line: optional_regex(&def.id, unless_line.as_deref())?,
                        except: optional_regex(&def.id, except.as_deref())?,
                        within: optional_regex(&def.id, within.as_deref())?,
                        unless_span: optional_regex(&def.id, unless_span.as_deref())?,
                    }),
                    Some(Check::RefsExist { pattern, suffixes }) => Matcher::RefsExist {
                        pattern: regex(&def.id, pattern)?,
                        suffixes: suffixes.clone(),
                    },
                    Some(Check::Require { pattern }) => Matcher::Require {
                        pattern: regex(&def.id, pattern)?,
                    },
                    Some(Check::MaxLines { max }) => Matcher::MaxLines { max: *max },
                    None => Matcher::None,
                };
                Ok(Compiled {
                    include: glob_set(&def.id, &def.files)?,
                    exclude: glob_set(&def.id, &def.exclude)?,
                    only_if: optional_regex(&def.id, def.only_if.as_deref())?,
                    unless: optional_regex(&def.id, def.unless.as_deref())?,
                    def: def.clone(),
                    matcher,
                })
            })
            .collect::<RulesResult<Vec<_>>>()?;
        Ok(Self { rules })
    }

    #[must_use]
    pub fn rules_for(&self, rel: &str) -> Vec<&RuleDef> {
        self.rules
            .iter()
            .filter(|r| r.applies(rel))
            .map(|r| &r.def)
            .collect()
    }

    /// Checks one file's text. Rules that need the repository on disk
    /// (`refs_exist`) are skipped; use [`Engine::check_text_at`] for those.
    #[must_use]
    pub fn check_text(&self, rel: &str, text: &str) -> Vec<Finding> {
        self.check_text_at(None, rel, text)
    }

    /// Like [`Engine::check_text`], resolving `refs_exist` paths against `root`.
    #[must_use]
    pub fn check_text_at(&self, root: Option<&Path>, rel: &str, text: &str) -> Vec<Finding> {
        let ctx = FileCtx {
            root,
            text,
            lines: text.lines().collect(),
            starts: line_starts(text),
        };
        self.rules
            .iter()
            .filter(|r| r.applies(rel) && r.passes_file_guards(text))
            .flat_map(|rule| {
                rule.hits(&ctx).into_iter().map(|hit| Finding {
                    rule: rule.def.id.clone(),
                    severity: rule.def.severity,
                    path: rel.to_string(),
                    line: hit.line,
                    column: hit.column,
                    message: rule.def.description.clone(),
                    fix: rule.def.fix.clone(),
                    snippet: snippet(&hit.snippet),
                })
            })
            .collect()
    }

    pub fn check_tree(&self, root: &Path, only: &[PathBuf]) -> Report {
        let files: Vec<(PathBuf, String)> = ignore::WalkBuilder::new(root)
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
            .filter(|(path, _)| only.is_empty() || only.iter().any(|o| path.starts_with(o)))
            .filter(|(_, rel)| self.rules.iter().any(|r| r.applies(rel)))
            .collect();

        let mut findings: Vec<Finding> = files
            .par_iter()
            .filter(|(path, _)| fs::metadata(path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES))
            .filter_map(|(path, rel)| {
                let text = fs::read_to_string(path).ok()?;
                Some(self.check_text_at(Some(root), rel, &text))
            })
            .flatten()
            .collect();
        findings.sort_by(|a, b| (&a.path, a.line, &a.rule).cmp(&(&b.path, b.line, &b.rule)));

        Report {
            files_scanned: files.len(),
            rules: self.rules.len(),
            findings,
        }
    }
}
