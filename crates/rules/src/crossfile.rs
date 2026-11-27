//! Matchers that look beyond one regex over one file: references that must
//! resolve, balanced blocks, symbols declared here and used elsewhere, and
//! sets that one file must cover from another.

use crate::ctx::{
    Cache, FileCtx, Hit, Repo, expand, has_path_placeholder, is_suppressed, locate, optional_regex,
    regex, scope,
};
use crate::error::RulesResult;
use crate::markup::skip_group;
use crate::model::SymbolUse;
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) struct RefsSpec {
    pattern: Option<Regex>,
    within: Option<Regex>,
    json_keys: Option<String>,
    template: Option<String>,
    glob: bool,
    suffixes: Vec<String>,
}

impl RefsSpec {
    pub(crate) fn new(
        rule: &str,
        pattern: &str,
        within: Option<&str>,
        json_keys: Option<&str>,
        template: Option<&str>,
        glob: bool,
        suffixes: &[String],
    ) -> RulesResult<Self> {
        Ok(Self {
            pattern: if pattern.is_empty() {
                None
            } else {
                Some(regex(rule, pattern)?)
            },
            within: optional_regex(rule, within)?,
            json_keys: json_keys.map(str::to_string),
            template: template.map(str::to_string),
            glob,
            suffixes: suffixes.to_vec(),
        })
    }

    /// Offsets and resolved references of this file.
    fn references(&self, ctx: &FileCtx<'_>) -> Vec<(usize, String)> {
        if let Some(path) = &self.json_keys {
            return json_keys(ctx.text, path)
                .into_iter()
                .map(|(offset, key)| (offset, self.resolve(&key, None, ctx.rel)))
                .collect();
        }
        let Some(pattern) = &self.pattern else {
            return Vec::new();
        };
        let mut found = Vec::new();
        let mut collect = |haystack: &str, base: usize| {
            for caps in pattern.captures_iter(haystack) {
                if let Some(reference) = scope(pattern, &caps) {
                    found.push((
                        base + reference.start(),
                        self.resolve(reference.as_str(), Some(&caps), ctx.rel),
                    ));
                }
            }
        };
        if let Some(within) = &self.within {
            for caps in within.captures_iter(ctx.text) {
                if let Some(span) = scope(within, &caps) {
                    collect(span.as_str(), span.start());
                }
            }
        } else {
            collect(ctx.text, 0);
        }
        found
    }

    fn resolve(&self, reference: &str, caps: Option<&regex::Captures<'_>>, rel: &str) -> String {
        self.template.as_deref().map_or_else(
            || reference.to_string(),
            |template| {
                let with_zero = template.replace("{0}", reference);
                expand(&with_zero, caps, rel, false)
            },
        )
    }

    fn exists(&self, repo: &Repo<'_>, reference: &str) -> bool {
        if self.glob {
            return Glob::new(reference).is_ok_and(|glob| {
                let matcher = glob.compile_matcher();
                repo.files().iter().any(|file| matcher.is_match(file))
            });
        }
        std::iter::once(String::new())
            .chain(self.suffixes.iter().cloned())
            .any(|suffix| repo.root().join(format!("{reference}{suffix}")).exists())
    }

    pub(crate) fn hits(&self, ctx: &FileCtx<'_>, rule: &str) -> Vec<Hit> {
        let Some(repo) = ctx.repo else {
            return Vec::new();
        };
        self.references(ctx)
            .into_iter()
            .filter_map(|(offset, reference)| {
                let (line, _) = locate(&ctx.starts, offset);
                let missing =
                    !self.exists(repo, &reference) && !is_suppressed(&ctx.lines, line - 1, rule);
                missing.then(|| ctx.hit_at_offset(offset))
            })
            .collect()
    }
}

/// Keys of the object at the dotted `path` of a JSON document, with the byte
/// offset where each key is written. Invalid JSON or a missing path: nothing.
fn json_keys(text: &str, path: &str) -> Vec<(usize, String)> {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let mut search_from = 0;
    for segment in path.split('.') {
        let Some(next) = value.get_mut(segment).map(serde_json::Value::take) else {
            return Vec::new();
        };
        search_from += text[search_from..]
            .find(&format!("\"{segment}\""))
            .unwrap_or(0);
        value = next;
    }
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    object
        .keys()
        .map(|key| {
            let offset = text[search_from..]
                .find(&format!("\"{key}\""))
                .map_or(search_from, |at| search_from + at + 1);
            (offset, key.clone())
        })
        .collect()
}

pub(crate) struct BlockSpec {
    open: Regex,
    end: Option<Regex>,
    when: Option<Regex>,
    require: Option<Regex>,
    forbid: Option<Regex>,
}

impl BlockSpec {
    pub(crate) fn new(
        rule: &str,
        open: &str,
        end: Option<&str>,
        when: Option<&str>,
        require: Option<&str>,
        forbid: Option<&str>,
    ) -> RulesResult<Self> {
        Ok(Self {
            open: regex(rule, open)?,
            end: optional_regex(rule, end)?,
            when: optional_regex(rule, when)?,
            require: optional_regex(rule, require)?,
            forbid: optional_regex(rule, forbid)?,
        })
    }

    fn body_end(&self, text: &str, open_end: usize) -> Option<usize> {
        if let Some(end) = &self.end {
            return Some(
                end.find(&text[open_end..])
                    .map_or(text.len(), |m| open_end + m.start()),
            );
        }
        let opener = open_end.checked_sub(1)?;
        matches!(text.as_bytes()[opener], b'(' | b'[' | b'{')
            .then(|| skip_group(text, opener).saturating_sub(1).max(open_end))
    }

    pub(crate) fn hits(&self, ctx: &FileCtx<'_>, rule: &str) -> Vec<Hit> {
        let mut hits = Vec::new();
        for opened in self.open.find_iter(ctx.text) {
            let Some(end) = self.body_end(ctx.text, opened.end()) else {
                continue;
            };
            let body = &ctx.text[opened.end()..end.min(ctx.text.len())];
            if self.when.as_ref().is_some_and(|w| !w.is_match(body)) {
                continue;
            }
            let (line, _) = locate(&ctx.starts, opened.start());
            if is_suppressed(&ctx.lines, line - 1, rule) {
                continue;
            }
            if self.require.as_ref().is_some_and(|r| !r.is_match(body)) {
                hits.push(ctx.hit_at_offset(opened.start()));
            }
            if let Some(found) = self.forbid.as_ref().and_then(|f| f.find(body)) {
                hits.push(ctx.hit_at_offset(opened.end() + found.start()));
            }
        }
        hits
    }
}

struct Use {
    pattern: Regex,
    symbol: String,
    own_file: bool,
}

pub(crate) struct SymbolsSpec {
    declare: Regex,
    within: Option<Regex>,
    corpus: GlobSet,
    uses: Vec<Use>,
}

impl SymbolsSpec {
    pub(crate) fn new(
        rule: &str,
        declare: &str,
        within: Option<&str>,
        corpus: &[String],
        uses: &[SymbolUse],
    ) -> RulesResult<Self> {
        let mut builder = GlobSetBuilder::new();
        for glob in corpus {
            builder.add(
                Glob::new(glob).map_err(|source| crate::error::RulesError::Glob {
                    rule: rule.to_string(),
                    source,
                })?,
            );
        }
        Ok(Self {
            declare: regex(rule, declare)?,
            within: optional_regex(rule, within)?,
            corpus: builder
                .build()
                .map_err(|source| crate::error::RulesError::Glob {
                    rule: rule.to_string(),
                    source,
                })?,
            uses: uses
                .iter()
                .map(|u| {
                    Ok(Use {
                        pattern: regex(rule, &u.pattern)?,
                        symbol: u.symbol.clone().unwrap_or_else(|| "{1}".to_string()),
                        own_file: u.own_file,
                    })
                })
                .collect::<RulesResult<_>>()?,
        })
    }

    fn collect(&self, text: &str, own_file: bool, into: &mut HashSet<String>) {
        for used in self.uses.iter().filter(|u| u.own_file == own_file) {
            for caps in used.pattern.captures_iter(text) {
                into.insert(expand(&used.symbol, Some(&caps), "", false));
            }
        }
    }

    fn corpus_symbols(&self, repo: &Repo<'_>) -> HashSet<String> {
        repo.files()
            .par_iter()
            .filter(|rel| self.corpus.is_match(rel.as_str()))
            .map(|rel| {
                let mut found = HashSet::new();
                let path = repo.root().join(rel);
                if fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES)
                    && let Ok(text) = fs::read_to_string(&path)
                {
                    self.collect(&text, false, &mut found);
                }
                found
            })
            .reduce(HashSet::new, |mut a, b| {
                a.extend(b);
                a
            })
    }

    fn declared(&self, ctx: &FileCtx<'_>) -> Vec<(usize, String)> {
        let mut found = Vec::new();
        let mut collect = |haystack: &str, base: usize| {
            for caps in self.declare.captures_iter(haystack) {
                if let Some(name) = scope(&self.declare, &caps) {
                    found.push((base + name.start(), name.as_str().to_string()));
                }
            }
        };
        if let Some(within) = &self.within {
            for caps in within.captures_iter(ctx.text) {
                if let Some(span) = scope(within, &caps) {
                    collect(span.as_str(), span.start());
                }
            }
        } else {
            collect(ctx.text, 0);
        }
        found
    }

    pub(crate) fn hits(&self, ctx: &FileCtx<'_>, rule_index: usize, rule: &str) -> Vec<Hit> {
        let Some(repo) = ctx.repo else {
            return Vec::new();
        };
        let declared = self.declared(ctx);
        if declared.is_empty() {
            return Vec::new();
        }
        let Some(cell) = repo.cache(rule_index) else {
            return Vec::new();
        };
        let Cache::Symbols(corpus) = cell.get_or_init(|| Cache::Symbols(self.corpus_symbols(repo)))
        else {
            return Vec::new();
        };
        let mut own = HashSet::new();
        self.collect(ctx.text, true, &mut own);
        declared
            .into_iter()
            .filter(|(offset, name)| {
                let (line, _) = locate(&ctx.starts, *offset);
                !corpus.contains(name)
                    && !own.contains(name)
                    && !is_suppressed(&ctx.lines, line - 1, rule)
            })
            .map(|(offset, _)| ctx.hit_at_offset(offset))
            .collect()
    }
}

pub(crate) struct CoveredSpec {
    pattern: Regex,
    in_files: Vec<String>,
    in_pattern: Regex,
    ignore: HashSet<String>,
}

impl CoveredSpec {
    pub(crate) fn new(
        rule: &str,
        pattern: &str,
        in_files: &[String],
        in_pattern: &str,
        ignore: &[String],
    ) -> RulesResult<Self> {
        Ok(Self {
            pattern: regex(rule, pattern)?,
            in_files: in_files.to_vec(),
            in_pattern: regex(rule, in_pattern)?,
            ignore: ignore.iter().cloned().collect(),
        })
    }

    fn targets(&self, root: &Path) -> Option<HashSet<String>> {
        let text = self
            .in_files
            .iter()
            .find_map(|file| fs::read_to_string(root.join(file)).ok())?;
        Some(
            self.in_pattern
                .captures_iter(&text)
                .filter_map(|caps| scope(&self.in_pattern, &caps))
                .map(|m| m.as_str().to_string())
                .collect(),
        )
    }

    pub(crate) fn hits(&self, ctx: &FileCtx<'_>, rule_index: usize) -> Vec<Hit> {
        let Some(repo) = ctx.repo else {
            return Vec::new();
        };
        let Some(cell) = repo.cache(rule_index) else {
            return Vec::new();
        };
        let Cache::Symbols(targets) =
            cell.get_or_init(|| Cache::Symbols(self.targets(repo.root()).unwrap_or_default()))
        else {
            return Vec::new();
        };
        if targets.is_empty() {
            return Vec::new();
        }
        let mut seen = HashSet::new();
        self.pattern
            .captures_iter(ctx.text)
            .filter_map(|caps| scope(&self.pattern, &caps))
            .filter(|m| {
                !self.ignore.contains(m.as_str())
                    && !targets.contains(m.as_str())
                    && seen.insert(m.as_str().to_string())
            })
            .map(|m| ctx.hit_at_offset(m.start()))
            .collect()
    }
}

/// Whether the template mentions a path placeholder (`{parent}`, `{stem}`..).
pub(crate) fn is_path_template(pattern: &str) -> bool {
    has_path_placeholder(pattern)
}
