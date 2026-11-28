//! The `element` check: selects JSX elements and asserts something about them,
//! their ancestors, their children or their siblings.

use crate::ctx::{FileCtx, Hit, is_suppressed, locate, optional_regex, regex};
use crate::error::RulesResult;
use crate::expr;
use crate::markup::{Attr, Markup};
use crate::model::{AncestorFilter, ChildValues, Compare, ElementCheck, Relation, Scope};
use regex::Regex;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

const CONST_WINDOW: usize = 2000;

struct AncestorSpec {
    tag: Option<Regex>,
    open: Option<Regex>,
    open_not: Option<Regex>,
    nearest: bool,
    within_chars: Option<usize>,
}

struct CompareSpec {
    to: Relation,
    related_tag: Option<Regex>,
    own: Regex,
    own_default: Option<String>,
    related: Regex,
    related_default: Option<String>,
    single: bool,
    allow: BTreeMap<String, Vec<String>>,
    deny: BTreeMap<String, Vec<String>>,
}

struct ChildSpec {
    tag: Regex,
    pattern: Regex,
    default: Option<String>,
    max: usize,
}

pub(crate) struct ElementSpec {
    tag: Option<Regex>,
    open: Vec<Regex>,
    open_not: Vec<Regex>,
    content: Option<Regex>,
    content_except: Option<Regex>,
    ancestor: Option<AncestorSpec>,
    class_conditional: Option<bool>,
    resolve_consts: bool,
    pattern: Option<Regex>,
    scope: Scope,
    except: Option<Regex>,
    distinct: Option<(Regex, usize)>,
    compare: Option<CompareSpec>,
    child_values: Option<ChildSpec>,
}

fn whole(rule: &str, pattern: &str) -> RulesResult<Regex> {
    regex(rule, &format!("^(?:{pattern})$"))
}

fn ancestor(rule: &str, filter: &AncestorFilter) -> RulesResult<AncestorSpec> {
    Ok(AncestorSpec {
        tag: filter.tag.as_deref().map(|t| whole(rule, t)).transpose()?,
        open: optional_regex(rule, filter.open.as_deref())?,
        open_not: optional_regex(rule, filter.open_not.as_deref())?,
        nearest: filter.nearest,
        within_chars: filter.within_chars,
    })
}

fn compare(rule: &str, spec: &Compare) -> RulesResult<CompareSpec> {
    Ok(CompareSpec {
        to: spec.to,
        related_tag: spec
            .related_tag
            .as_deref()
            .map(|t| whole(rule, t))
            .transpose()?,
        own: regex(rule, &spec.own)?,
        own_default: spec.own_default.clone(),
        related: regex(rule, &spec.related)?,
        related_default: spec.related_default.clone(),
        single: spec.single,
        allow: spec.allow.clone(),
        deny: spec.deny.clone(),
    })
}

fn children(rule: &str, spec: &ChildValues) -> RulesResult<ChildSpec> {
    Ok(ChildSpec {
        tag: whole(rule, &spec.tag)?,
        pattern: regex(rule, &spec.pattern)?,
        default: spec.default.clone(),
        max: spec.max,
    })
}

impl ElementSpec {
    pub(crate) fn new(rule: &str, check: &ElementCheck) -> RulesResult<Self> {
        Ok(Self {
            tag: check.tag.as_deref().map(|t| whole(rule, t)).transpose()?,
            open: check
                .open
                .iter()
                .map(|p| regex(rule, p))
                .collect::<RulesResult<_>>()?,
            open_not: check
                .open_not
                .iter()
                .map(|p| regex(rule, p))
                .collect::<RulesResult<_>>()?,
            content: optional_regex(rule, check.content.as_deref())?,
            content_except: optional_regex(rule, check.content_except.as_deref())?,
            ancestor: check
                .ancestor
                .as_ref()
                .map(|a| ancestor(rule, a))
                .transpose()?,
            class_conditional: check.class_conditional,
            resolve_consts: check.resolve_consts,
            pattern: optional_regex(rule, check.pattern.as_deref())?,
            scope: check.scope,
            except: optional_regex(rule, check.except.as_deref())?,
            distinct: check
                .distinct
                .as_ref()
                .map(|d| Ok((regex(rule, &d.pattern)?, d.max)))
                .transpose()?,
            compare: check
                .compare
                .as_ref()
                .map(|c| compare(rule, c))
                .transpose()?,
            child_values: check
                .child_values
                .as_ref()
                .map(|c| children(rule, c))
                .transpose()?,
        })
    }

    pub(crate) fn hits(&self, ctx: &FileCtx<'_>, rule: &str) -> Vec<Hit> {
        if !ctx.text.contains('<') {
            return Vec::new();
        }
        let markup = ctx.markup();
        let mut hits = Vec::new();
        for index in 0..markup.elements.len() {
            let element = &markup.elements[index];
            let (line, _) = locate(&ctx.starts, element.open.start);
            if self
                .tag
                .as_ref()
                .is_some_and(|t| !t.is_match(&element.name))
                || is_suppressed(&ctx.lines, line - 1, rule)
            {
                continue;
            }
            let open = markup.open_text(ctx.text, index);
            let haystack = self.haystack(ctx, markup, index, open);
            if !self.selects(ctx, markup, index, &haystack) {
                continue;
            }
            self.assert(ctx, markup, index, &haystack, &mut hits);
        }
        hits
    }

    /// The opening tag, plus the initializers of the variables its
    /// attributes point to when `resolve_consts` is on.
    fn haystack<'a>(
        &self,
        ctx: &FileCtx<'_>,
        markup: &Markup,
        index: usize,
        open: &'a str,
    ) -> Cow<'a, str> {
        if !self.resolve_consts {
            return Cow::Borrowed(open);
        }
        let mut text = open.to_string();
        for attr in markup.attrs(ctx.text, index) {
            let Some(range) = attr.value else { continue };
            let raw = ctx.text[range].trim();
            let Some(name) = raw
                .strip_prefix('{')
                .and_then(|r| r.strip_suffix('}'))
                .map(str::trim)
                .filter(|n| is_identifier(n))
            else {
                continue;
            };
            if let Some(init) =
                const_initializer(&ctx.text[..markup.elements[index].open.start], name)
            {
                text.push('\n');
                text.push_str(init);
            }
        }
        Cow::Owned(text)
    }

    fn selects(&self, ctx: &FileCtx<'_>, markup: &Markup, index: usize, haystack: &str) -> bool {
        if !self.open.iter().all(|re| re.is_match(haystack))
            || self.open_not.iter().any(|re| re.is_match(haystack))
        {
            return false;
        }
        if let Some(content) = &self.content {
            let inner = markup.inner_text(ctx.text, index);
            let Some(caps) = content.captures(inner) else {
                return false;
            };
            let captured = caps
                .get(1)
                .or_else(|| caps.get(0))
                .map_or("", |m| m.as_str());
            if self
                .content_except
                .as_ref()
                .is_some_and(|re| re.is_match(captured))
            {
                return false;
            }
        }
        if let Some(wanted) = self.class_conditional
            && class_is_conditional(ctx.text, markup, index) != wanted
        {
            return false;
        }
        self.ancestor
            .as_ref()
            .is_none_or(|spec| has_ancestor(ctx.text, markup, index, spec))
    }

    fn assert(
        &self,
        ctx: &FileCtx<'_>,
        markup: &Markup,
        index: usize,
        haystack: &str,
        hits: &mut Vec<Hit>,
    ) {
        let element = &markup.elements[index];
        let start = element.open.start;
        if let Some(pattern) = &self.pattern {
            let (text, base, limit) = self.scoped_text(ctx, markup, index, haystack);
            for found in pattern.find_iter(&text) {
                if self
                    .except
                    .as_ref()
                    .is_some_and(|re| re.is_match(found.as_str()))
                {
                    continue;
                }
                let offset = if found.start() < limit {
                    base + found.start()
                } else {
                    start
                };
                hits.push(ctx.hit_at_offset(offset));
            }
        } else if let Some((pattern, max)) = &self.distinct {
            let values: BTreeSet<&str> = pattern
                .captures_iter(haystack)
                .filter_map(|c| c.get(1).or_else(|| c.get(0)))
                .map(|m| m.as_str())
                .collect();
            if values.len() > *max {
                hits.push(ctx.hit_at_offset(start));
            }
        } else if let Some(spec) = &self.compare {
            if Self::mismatch(ctx, markup, index, haystack, spec) {
                hits.push(ctx.hit_at_offset(start));
            }
        } else if let Some(spec) = &self.child_values {
            let values: BTreeSet<String> = element
                .children
                .iter()
                .filter(|&&c| spec.tag.is_match(&markup.elements[c].name))
                .filter_map(|&c| {
                    first_capture(markup.open_text(ctx.text, c), &spec.pattern)
                        .or_else(|| spec.default.clone())
                })
                .collect();
            if values.len() > spec.max {
                hits.push(ctx.hit_at_offset(start));
            }
        } else {
            hits.push(ctx.hit_at_offset(start));
        }
    }

    /// The text `pattern` searches, where it starts in the file and how many
    /// of its leading bytes really are file text (the rest is resolved constants).
    fn scoped_text<'a>(
        &self,
        ctx: &'a FileCtx<'_>,
        markup: &Markup,
        index: usize,
        haystack: &'a str,
    ) -> (Cow<'a, str>, usize, usize) {
        let element = &markup.elements[index];
        match self.scope {
            Scope::Open => (
                Cow::Borrowed(haystack),
                element.open.start,
                element.open.len(),
            ),
            Scope::Content => (
                Cow::Borrowed(markup.inner_text(ctx.text, index)),
                element.inner.start,
                element.inner.len(),
            ),
            Scope::ClassStrings => (
                Cow::Owned(class_strings(ctx.text, markup, index)),
                element.open.start,
                0,
            ),
            Scope::Class => class_attr(ctx.text, markup, index)
                .and_then(|attr| attr.value)
                .map_or((Cow::Borrowed(""), element.open.start, 0), |range| {
                    let len = range.len();
                    (Cow::Borrowed(&ctx.text[range.clone()]), range.start, len)
                }),
        }
    }

    fn mismatch(
        ctx: &FileCtx<'_>,
        markup: &Markup,
        index: usize,
        haystack: &str,
        spec: &CompareSpec,
    ) -> bool {
        let own_values: BTreeSet<String> = spec
            .own
            .captures_iter(haystack)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        if spec.single && own_values.len() > 1 {
            return false;
        }
        let Some(own) = own_values
            .into_iter()
            .next()
            .or_else(|| spec.own_default.clone())
        else {
            return false;
        };
        let related = Self::related_value(ctx, markup, index, spec);
        let Some(related) = related else { return false };
        let allowed_wrong = spec
            .allow
            .get(&related)
            .is_some_and(|allowed| !allowed.contains(&own));
        let denied = spec
            .deny
            .get(&related)
            .is_some_and(|denied| denied.contains(&own));
        allowed_wrong || denied
    }

    fn related_value(
        ctx: &FileCtx<'_>,
        markup: &Markup,
        index: usize,
        spec: &CompareSpec,
    ) -> Option<String> {
        let element = &markup.elements[index];
        let candidates: Vec<usize> = match spec.to {
            Relation::Parent => element.parent.into_iter().collect(),
            Relation::Sibling => element.parent.map_or_else(Vec::new, |p| {
                markup.elements[p]
                    .children
                    .iter()
                    .copied()
                    .filter(|&c| c != index)
                    .collect()
            }),
        };
        candidates
            .into_iter()
            .filter(|&c| {
                spec.related_tag
                    .as_ref()
                    .is_none_or(|t| t.is_match(&markup.elements[c].name))
            })
            .find_map(|c| {
                first_capture(markup.open_text(ctx.text, c), &spec.related)
                    .or_else(|| spec.related_default.clone())
            })
    }
}

fn first_capture(text: &str, pattern: &Regex) -> Option<String> {
    pattern
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// The text assigned by the closest `const name = ...;` before `before`.
fn const_initializer<'a>(before: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("const {name}");
    let mut search_end = before.len();
    while let Some(at) = before[..search_end].rfind(&needle) {
        let rest = &before[at + needle.len()..];
        if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
            search_end = at;
            continue;
        }
        let eq = rest.find('=')?;
        let init = &rest[eq + 1..];
        let end = init.find(';').unwrap_or(init.len()).min(CONST_WINDOW);
        return init.get(..end).or(Some(init));
    }
    None
}

fn class_attr(src: &str, markup: &Markup, index: usize) -> Option<Attr> {
    markup
        .attrs(src, index)
        .into_iter()
        .find(|a| a.name == "className" || a.name == "class")
}

fn class_is_conditional(src: &str, markup: &Markup, index: usize) -> bool {
    let Some(range) = class_attr(src, markup, index).and_then(|a| a.value) else {
        return false;
    };
    let raw = src[range].trim();
    raw.strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .is_some_and(|inner| expr::is_conditional(&expr::parse(inner)))
}

/// The literal class strings of an element, joined with spaces.
fn class_strings(src: &str, markup: &Markup, index: usize) -> String {
    let Some(range) = class_attr(src, markup, index).and_then(|a| a.value) else {
        return String::new();
    };
    let raw = src[range].trim();
    match raw.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some(inner) => expr::class_text(&expr::parse(inner)),
        None => raw.trim_matches(['"', '\'']).to_string(),
    }
}

fn has_ancestor(src: &str, markup: &Markup, index: usize, spec: &AncestorSpec) -> bool {
    let start = markup.elements[index].open.start;
    let matches = |a: usize| {
        let open = markup.open_text(src, a);
        spec.within_chars
            .is_none_or(|limit| start - markup.elements[a].open.start <= limit)
            && spec
                .tag
                .as_ref()
                .is_none_or(|t| t.is_match(&markup.elements[a].name))
            && spec.open.as_ref().is_none_or(|re| re.is_match(open))
            && spec.open_not.as_ref().is_none_or(|re| !re.is_match(open))
    };
    if spec.nearest {
        markup.ancestors(index).next().is_some_and(matches)
    } else {
        markup.ancestors(index).any(matches)
    }
}
