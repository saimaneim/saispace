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

enum Matcher {
    Forbid {
        pattern: Regex,
        stop_at: Option<Regex>,
    },
    Require {
        pattern: Regex,
    },
    MaxLines {
        max: usize,
    },
    None,
}

struct Compiled {
    def: RuleDef,
    include: GlobSet,
    exclude: GlobSet,
    matcher: Matcher,
}

impl Compiled {
    fn applies(&self, rel: &str) -> bool {
        self.include.is_match(rel) && !self.exclude.is_match(rel)
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

fn is_suppressed(lines: &[&str], index: usize, rule: &str) -> bool {
    let marker = |line: &str| {
        line.split("sai-ignore:")
            .nth(1)
            .is_some_and(|rest| rest.split([',', ' ']).any(|id| id.trim() == rule))
    };
    marker(lines[index]) || (index > 0 && marker(lines[index - 1]))
}

fn snippet(line: &str) -> String {
    line.trim().chars().take(SNIPPET_CHARS).collect()
}

impl Engine {
    pub fn new(defs: &[RuleDef]) -> RulesResult<Self> {
        let rules = defs
            .iter()
            .map(|def| {
                def.validate()?;
                let matcher = match &def.check {
                    Some(Check::Forbid { pattern, stop_at }) => Matcher::Forbid {
                        pattern: regex(&def.id, pattern)?,
                        stop_at: stop_at.as_deref().map(|s| regex(&def.id, s)).transpose()?,
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

    #[must_use]
    pub fn check_text(&self, rel: &str, text: &str) -> Vec<Finding> {
        let lines: Vec<&str> = text.lines().collect();
        let mut findings = Vec::new();
        for rule in self.rules.iter().filter(|r| r.applies(rel)) {
            let finding = |line: usize, column: usize, snippet_text: &str| Finding {
                rule: rule.def.id.clone(),
                severity: rule.def.severity,
                path: rel.to_string(),
                line,
                column,
                message: rule.def.description.clone(),
                fix: rule.def.fix.clone(),
                snippet: snippet(snippet_text),
            };
            match &rule.matcher {
                Matcher::Forbid { pattern, stop_at } => {
                    for (index, line) in lines.iter().enumerate() {
                        if stop_at.as_ref().is_some_and(|stop| stop.is_match(line)) {
                            break;
                        }
                        if let Some(m) = pattern.find(line)
                            && !is_suppressed(&lines, index, &rule.def.id)
                        {
                            findings.push(finding(index + 1, m.start() + 1, line));
                        }
                    }
                }
                Matcher::Require { pattern } => {
                    if !pattern.is_match(text) {
                        findings.push(finding(1, 1, lines.first().copied().unwrap_or_default()));
                    }
                }
                Matcher::MaxLines { max } => {
                    if lines.len() > *max {
                        let note = format!("{} lines (max {max})", lines.len());
                        findings.push(finding(*max + 1, 1, &note));
                    }
                }
                Matcher::None => {}
            }
        }
        findings
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
                Some(self.check_text(rel, &text))
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
