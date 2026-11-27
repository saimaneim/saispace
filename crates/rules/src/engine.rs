use crate::contrast::ContrastSpec;
use crate::crossfile::{BlockSpec, CoveredSpec, RefsSpec, SymbolsSpec, is_path_template};
use crate::csscheck::CssSpec;
use crate::ctx::{
    FileCtx, Hit, Repo, expand, is_suppressed, locate, optional_regex, regex, scope, snippet, walk,
};
use crate::element::ElementSpec;
use crate::error::{RulesError, RulesResult};
use crate::model::{Check, RuleDef, Severity};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

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
    unless_before: Option<Regex>,
    before_chars: usize,
}

/// A `require` pattern; when it names the file (`{parent}`, `{stem}`..) it is
/// built per file.
enum RequireSpec {
    Fixed(Regex),
    PerFile(String),
}

enum Matcher {
    Forbid(ForbidSpec),
    RefsExist(RefsSpec),
    Require(RequireSpec),
    MaxLines { max: usize },
    Block(BlockSpec),
    Symbols(SymbolsSpec),
    Covered(CoveredSpec),
    Element(Box<ElementSpec>),
    Contrast(Box<ContrastSpec>),
    Css(CssSpec),
    None,
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
        // Multiline and scoped searches see only the text before `stop_at`.
        let searched = ctx
            .starts
            .get(stop_line)
            .map_or(ctx.text, |&end| &ctx.text[..end]);
        let after_guard = |offset: usize| {
            self.unless_before.as_ref().is_some_and(|re| {
                let from = (offset.saturating_sub(self.before_chars)..=offset)
                    .find(|&i| ctx.text.is_char_boundary(i))
                    .unwrap_or(offset);
                re.is_match(&ctx.text[from..offset])
            })
        };
        let mut hits = Vec::new();
        if let Some(within) = &self.within {
            for caps in within.captures_iter(searched) {
                let (Some(whole), Some(span)) = (caps.get(0), scope(within, &caps)) else {
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
                    let offset = span.start() + m.start();
                    let (line, column) = locate(&ctx.starts, offset);
                    if line > stop_line {
                        break;
                    }
                    if !skipped(&m, line) && !after_guard(offset) {
                        hits.push(Hit::at(ctx, line, column));
                    }
                }
            }
        } else if self.multiline {
            for m in self.pattern.find_iter(searched) {
                let (line, column) = locate(&ctx.starts, m.start());
                if line > stop_line {
                    break;
                }
                if !skipped(&m, line) && !after_guard(m.start()) {
                    hits.push(Hit::at(ctx, line, column));
                }
            }
        } else {
            for (index, line) in ctx.lines.iter().enumerate().take(stop_line) {
                let found = self.pattern.find_iter(line).find(|m| {
                    !skipped(m, index + 1) && !after_guard(ctx.starts[index] + m.start())
                });
                if let Some(m) = found {
                    hits.push(Hit::at(ctx, index + 1, m.start() + 1));
                }
            }
        }
        hits
    }
}

impl Compiled {
    fn hits(&self, ctx: &FileCtx<'_>, index: usize) -> Vec<Hit> {
        let rule = &self.def.id;
        match &self.matcher {
            Matcher::Forbid(spec) => spec.hits(ctx, rule),
            Matcher::RefsExist(spec) => spec.hits(ctx, rule),
            Matcher::Require(spec) => {
                let found = match spec {
                    RequireSpec::Fixed(re) => re.is_match(ctx.text),
                    RequireSpec::PerFile(template) => {
                        Regex::new(&expand(template, None, ctx.rel, true))
                            .is_ok_and(|re| re.is_match(ctx.text))
                    }
                };
                if found {
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
            Matcher::Block(spec) => spec.hits(ctx, rule),
            Matcher::Symbols(spec) => spec.hits(ctx, index, rule),
            Matcher::Covered(spec) => spec.hits(ctx, index),
            Matcher::Element(spec) => spec.hits(ctx, rule),
            Matcher::Contrast(spec) => spec.hits(ctx, index),
            Matcher::Css(spec) => spec.hits(ctx),
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

fn compile_matcher(def: &RuleDef) -> RulesResult<Matcher> {
    let id = def.id.as_str();
    Ok(match &def.check {
        Some(Check::Forbid {
            pattern,
            stop_at,
            multiline,
            unless_line,
            except,
            within,
            unless_span,
            unless_before,
            before_chars,
        }) => Matcher::Forbid(ForbidSpec {
            pattern: regex(id, pattern)?,
            stop_at: optional_regex(id, stop_at.as_deref())?,
            multiline: *multiline,
            unless_line: optional_regex(id, unless_line.as_deref())?,
            except: optional_regex(id, except.as_deref())?,
            within: optional_regex(id, within.as_deref())?,
            unless_span: optional_regex(id, unless_span.as_deref())?,
            unless_before: optional_regex(id, unless_before.as_deref())?,
            before_chars: before_chars.unwrap_or(80),
        }),
        Some(Check::RefsExist {
            pattern,
            within,
            json_keys,
            template,
            glob,
            suffixes,
        }) => Matcher::RefsExist(RefsSpec::new(
            id,
            pattern,
            within.as_deref(),
            json_keys.as_deref(),
            template.as_deref(),
            *glob,
            suffixes,
        )?),
        Some(Check::Require { pattern }) if is_path_template(pattern) => {
            regex(id, &expand(pattern, None, "a/b/c.d", true))?;
            Matcher::Require(RequireSpec::PerFile(pattern.clone()))
        }
        Some(Check::Require { pattern }) => {
            Matcher::Require(RequireSpec::Fixed(regex(id, pattern)?))
        }
        Some(Check::MaxLines { max }) => Matcher::MaxLines { max: *max },
        Some(Check::Block {
            open,
            end,
            when,
            require,
            forbid,
        }) => Matcher::Block(BlockSpec::new(
            id,
            open,
            end.as_deref(),
            when.as_deref(),
            require.as_deref(),
            forbid.as_deref(),
        )?),
        Some(Check::UnusedSymbols {
            declare,
            within,
            corpus,
            uses,
        }) => Matcher::Symbols(SymbolsSpec::new(
            id,
            declare,
            within.as_deref(),
            corpus,
            uses,
        )?),
        Some(Check::SetCovered {
            pattern,
            in_files,
            in_pattern,
            ignore,
        }) => Matcher::Covered(CoveredSpec::new(id, pattern, in_files, in_pattern, ignore)?),
        Some(Check::Element(element)) => Matcher::Element(Box::new(ElementSpec::new(id, element)?)),
        Some(Check::Contrast(contrast)) => {
            Matcher::Contrast(Box::new(ContrastSpec::new(id, contrast)?))
        }
        Some(css) => match CssSpec::new(id, css)? {
            Some(spec) => Matcher::Css(spec),
            None => Matcher::None,
        },
        None => Matcher::None,
    })
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
                Ok(Compiled {
                    include: glob_set(&def.id, &def.files)?,
                    exclude: glob_set(&def.id, &def.exclude)?,
                    only_if: optional_regex(&def.id, def.only_if.as_deref())?,
                    unless: optional_regex(&def.id, def.unless.as_deref())?,
                    def: def.clone(),
                    matcher: compile_matcher(def)?,
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
    /// (`refs_exist`, `unused_symbols`, `contrast`..) are skipped; use
    /// [`Engine::check_text_at`] for those.
    #[must_use]
    pub fn check_text(&self, rel: &str, text: &str) -> Vec<Finding> {
        self.check_file(None, rel, text)
    }

    /// Like [`Engine::check_text`], resolving cross-file rules against `root`.
    #[must_use]
    pub fn check_text_at(&self, root: Option<&Path>, rel: &str, text: &str) -> Vec<Finding> {
        let repo = root.map(|r| Repo::new(r, self.rules.len(), None));
        self.check_file(repo.as_ref(), rel, text)
    }

    fn check_file(&self, repo: Option<&Repo<'_>>, rel: &str, text: &str) -> Vec<Finding> {
        let ctx = FileCtx::new(repo, rel, text);
        self.rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.applies(rel) && r.passes_file_guards(text))
            .flat_map(|(index, rule)| {
                rule.hits(&ctx, index).into_iter().map(|hit| Finding {
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

    #[must_use]
    pub fn check_tree(&self, root: &Path, only: &[PathBuf]) -> Report {
        let everything = walk(root);
        let repo = Repo::new(
            root,
            self.rules.len(),
            Some(everything.iter().map(|(_, rel)| rel.clone()).collect()),
        );
        let files: Vec<&(PathBuf, String)> = everything
            .iter()
            .filter(|(path, _)| only.is_empty() || only.iter().any(|o| path.starts_with(o)))
            .filter(|(_, rel)| self.rules.iter().any(|r| r.applies(rel)))
            .collect();

        let mut findings: Vec<Finding> = files
            .par_iter()
            .filter(|(path, _)| fs::metadata(path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES))
            .filter_map(|(path, rel)| {
                let text = fs::read_to_string(path).ok()?;
                Some(self.check_file(Some(&repo), rel, &text))
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
