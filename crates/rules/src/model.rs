use crate::error::{RulesError, RulesResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warn,
    #[default]
    Error,
}

impl Severity {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    Forbid {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_at: Option<String>,
        /// Match `pattern` against the whole file instead of line by line, so it
        /// can span lines. The finding sits on the line where the match starts.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        multiline: bool,
        /// Skip matches that start on a line this regex matches.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unless_line: Option<String>,
        /// Drop matches whose own text matches this regex.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        except: Option<String>,
        /// Only look inside the spans this regex matches (its capture group 1
        /// when it has one, the whole match otherwise), across lines.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<String>,
        /// With `within`: skip a span when the whole `within` match (not only its
        /// capture group 1) matches this regex, e.g. an opt-in attribute on the tag.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unless_span: Option<String>,
        /// Skip a match when this regex matches the `before_chars` characters
        /// (default 80) just before it, e.g. an enclosing `Object.entries(`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unless_before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_chars: Option<usize>,
    },
    Require {
        pattern: String,
    },
    MaxLines {
        max: usize,
    },
    /// Every reference must resolve. References come from the matches of
    /// `pattern` (capture group 1, or the whole match), optionally limited to
    /// the spans of `within`, or from the keys of a JSON object (`json_keys`,
    /// a dotted path such as `skills`). `template` turns a reference into the
    /// path to check: `{0}` the whole match, `{1}`.. captures, `{name}` named
    /// captures, and `{path}` `{dir}` `{parent}` `{file}` `{stem}` `{ext}` of
    /// the file being checked. Without `template` the reference is the path.
    /// With `glob = true` the reference is a glob that must match at least one
    /// file of the repository; otherwise the path must exist as is or with one
    /// of the `suffixes` appended.
    RefsExist {
        #[serde(default)]
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        json_keys: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        template: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        glob: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        suffixes: Vec<String>,
    },
    /// Scopes a search to balanced blocks. Every match of `open` starts a
    /// block: it ends at the first match of `end` after it, or, without `end`,
    /// at the delimiter that closes the bracket the `open` match ends with.
    /// Inside blocks where `when` matches (all blocks when absent), the body
    /// must match `require` and must not match `forbid`.
    Block {
        open: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        require: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        forbid: Option<String>,
    },
    /// Every symbol declared in this file (capture group 1 of `declare`,
    /// optionally inside the spans of `within`) must be used somewhere in the
    /// `corpus` files. The uses are collected in one pass with the `uses`
    /// regexes; each builds a symbol from its captures (`{1}`).
    UnusedSymbols {
        declare: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<String>,
        corpus: Vec<String>,
        uses: Vec<SymbolUse>,
    },
    /// Every value captured by `pattern` in this file must also be captured by
    /// `in_pattern` in the first existing file of `in_files`, unless listed in `ignore`.
    SetCovered {
        pattern: String,
        in_files: Vec<String>,
        in_pattern: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ignore: Vec<String>,
    },
    /// Selects JSX/TSX elements by tag, tag text, content, ancestors and
    /// class expression, then reports the element, the matches inside it, or a
    /// mismatch with a related element. See `ElementCheck`.
    Element(Box<ElementCheck>),
    /// WCAG contrast of the text/background utility classes of an element or
    /// of each variant of a `cva(...)` call. See `ContrastCheck`.
    Contrast(Box<ContrastCheck>),
    /// CSS custom properties whose name matches `names` must resolve to at
    /// least `min_px` (`rem` counts 16px).
    CssLength {
        names: String,
        min_px: f64,
    },
    /// For every size token matching `names`, `<name><suffix>` must resolve to
    /// a ratio of at least `min_ratio`, unless the size is `skip_from_px` or more.
    CssLineHeight {
        names: String,
        suffix: String,
        min_ratio: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skip_from_px: Option<f64>,
    },
    /// The tokens matching `names` form a scale: resolved to px they must be
    /// pairwise distinct and at least `min_step_px` apart once sorted.
    CssScale {
        names: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclude: Option<String>,
        min_step_px: f64,
    },
    /// Color tokens (resolved through `var()` chains) must keep a WCAG contrast.
    CssContrast {
        pairs: Vec<ColorPair>,
    },
    /// Every `var(--x)` inside a token matching `names` must point to a
    /// declared token.
    CssRefs {
        names: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolUse {
    pub pattern: String,
    /// Symbol built from the captures, e.g. `--{1}`. Defaults to `{1}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// `self` applies the regex to the declaring file instead of the corpus.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own_file: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorPair {
    pub fg: String,
    pub bg: String,
    pub min: f64,
    pub label: String,
}

/// Which relative of the element to compare with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Parent,
    Sibling,
}

/// A required ancestor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AncestorFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_not: Option<String>,
    /// Only the direct parent counts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nearest: bool,
    /// Only ancestors whose opening tag starts at most this many bytes before the element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Distinct {
    pub pattern: String,
    pub max: usize,
}

/// Compares a value read from the element with one read from a relative.
/// `allow` / `deny` map the related value to the accepted / rejected values of the element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compare {
    pub to: Relation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_tag: Option<String>,
    pub own: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_default: Option<String>,
    pub related: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_default: Option<String>,
    /// Skip unless the element carries exactly one distinct own value.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub single: bool,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub allow: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub deny: std::collections::BTreeMap<String, Vec<String>>,
}

/// The direct children named `tag` must not carry more than `max` distinct values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildValues {
    pub tag: String,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub max: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The opening tag.
    #[default]
    Open,
    /// The value of the `className` / `class` attribute, as written.
    Class,
    /// The string literals of the `className` / `class` expression joined with
    /// spaces: strings, the text parts (not the `${}` parts) of templates, both
    /// branches of ternaries, both sides of `&&`/`||`, call arguments and arrays.
    /// Matches are reported on the element's line.
    ClassStrings,
    /// The text between the opening and closing tags.
    Content,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementCheck {
    /// Regex on the tag name (matched whole).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Regexes the opening tag must all match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<String>,
    /// Regexes the opening tag must not match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_not: Vec<String>,
    /// Regex the inner text must match; its group 1 is the "captured text".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Skip when the captured text of `content` matches this regex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_except: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ancestor: Option<AncestorFilter>,
    /// `false` keeps only elements whose class expression is not conditional
    /// (no ternary, `&&`, `||`, `??`), `true` only the conditional ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_conditional: Option<bool>,
    /// Append to the opening tag the `const name = ...;` initializer of every
    /// `attr={name}` value so patterns also see class names kept in a variable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolve_consts: bool,
    /// Report each match of this regex inside `scope` (default: the element itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default)]
    pub scope: Scope,
    /// Drop matches of `pattern` whose text matches this regex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub except: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distinct: Option<Distinct>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<Compare>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_values: Option<ChildValues>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContrastUnits {
    /// Every element with direct text; colors cascade from ancestors.
    Jsx,
    /// Every entry of `group` in `call(base, { variants: { group: { ... } } })`.
    Variants,
}

fn default_min() -> f64 {
    4.5
}

fn default_min_large() -> f64 {
    3.0
}

fn default_call() -> String {
    "cva".to_string()
}

fn default_group() -> String {
    "variant".to_string()
}

fn default_max_variants() -> usize {
    12
}

fn default_skip_prefixes() -> Vec<String> {
    ["focus", "disabled", "aria", "data", "peer", "has", "in"]
        .map(str::to_string)
        .to_vec()
}

/// A sibling that paints the background behind the text (a moving highlight
/// pill, a selection indicator): when the element's text class matches
/// `when_fg` and a sibling carries a class matching `sibling`, the class
/// `apply` is added to the element's own classes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiblingBackground {
    pub when_fg: String,
    pub sibling: String,
    pub apply: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContrastCheck {
    pub units: ContrastUnits,
    /// CSS files whose custom properties define the colors: a class
    /// `bg-<name>` / `text-<name>` reads `--color-<name>` then `--<name>`.
    pub colors: Vec<String>,
    /// Text and background used when the classes set only one of them (`#rrggbb`).
    pub default_fg: String,
    pub default_bg: String,
    #[serde(default = "default_min")]
    pub min: f64,
    #[serde(default = "default_min_large")]
    pub min_large: f64,
    /// Minimum for hover, active and icon (`[&_svg]:`) colors.
    #[serde(default = "default_min_large")]
    pub min_state: f64,
    /// Class (without variants) that makes text large, e.g. `^text-(?:[2-9]?xl)$`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large: Option<String>,
    /// Large too when a class matches `large_size` and another one `bold`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<String>,
    /// Variants only: classes that lower the minimum to `min_emphasis`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emphasis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_emphasis: Option<f64>,
    /// Jsx only: skip elements whose own or ancestors' opening tag matches this regex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_open: Option<String>,
    /// Jsx only: see [`SiblingBackground`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_background: Option<SiblingBackground>,
    #[serde(default = "default_call")]
    pub call: String,
    #[serde(default = "default_group")]
    pub group: String,
    /// Class variants (`focus:`, `disabled:`..) that are not evaluated.
    #[serde(default = "default_skip_prefixes")]
    pub skip_prefixes: Vec<String>,
    #[serde(default = "default_max_variants")]
    pub max_variants: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleDef {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub severity: Severity,
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    /// File-level guard: the rule only runs on files whose text matches this regex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_if: Option<String>,
    /// File-level guard: the rule skips files whose text matches this regex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unless: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<String>,
}

impl RuleDef {
    /// # Errors
    ///
    /// Returns an error if a required field is empty or malformed.
    pub fn validate(&self) -> RulesResult<()> {
        let invalid = |msg: &str| Err(RulesError::Invalid(self.id.clone(), msg.to_string()));
        if !is_valid_id(&self.id) {
            return invalid("id must use lowercase letters, digits and dashes");
        }
        if self.description.trim().is_empty() {
            return invalid("description must not be empty");
        }
        if self.files.is_empty() {
            return invalid("files must contain at least one glob");
        }
        if self.files.iter().any(|g| g.trim().is_empty()) {
            return invalid("files must not contain empty globs");
        }
        if self.check.is_none() && self.guide.as_deref().is_none_or(|g| g.trim().is_empty()) {
            return invalid("a rule needs a check, a guide, or both");
        }
        let empty = |value: &Option<String>| value.as_deref().is_some_and(str::is_empty);
        if empty(&self.only_if) || empty(&self.unless) {
            return invalid("only_if and unless must not be empty");
        }
        validate_check(self.check.as_ref(), &self.id)
    }
}

fn validate_check(check: Option<&Check>, id: &str) -> RulesResult<()> {
    let invalid = |msg: &str| Err(RulesError::Invalid(id.to_string(), msg.to_string()));
    let empty = |value: &Option<String>| value.as_deref().is_some_and(str::is_empty);
    {
        match check {
            Some(Check::MaxLines { max: 0 }) => {
                return invalid("max_lines must be greater than zero");
            }
            Some(Check::Forbid { pattern, .. } | Check::Require { pattern })
                if pattern.is_empty() =>
            {
                return invalid("pattern must not be empty");
            }
            Some(Check::RefsExist {
                pattern,
                json_keys,
                within,
                ..
            }) => {
                if pattern.is_empty() == json_keys.is_none() {
                    return invalid("refs_exist needs exactly one of pattern or json_keys");
                }
                if empty(within) || empty(json_keys) {
                    return invalid("within and json_keys must not be empty");
                }
            }
            Some(Check::Forbid {
                stop_at,
                unless_line,
                except,
                within,
                unless_span,
                unless_before,
                ..
            }) if empty(stop_at)
                || empty(unless_line)
                || empty(except)
                || empty(within)
                || empty(unless_span)
                || empty(unless_before) =>
            {
                return invalid(
                    "stop_at, unless_line, except, within, unless_span and unless_before must not be empty",
                );
            }
            Some(Check::Forbid {
                within: None,
                unless_span: Some(_),
                ..
            }) => {
                return invalid("unless_span needs within");
            }
            Some(Check::Block { open, end, .. }) if open.is_empty() || empty(end) => {
                return invalid("open and end must not be empty");
            }
            Some(Check::Block {
                require: None,
                forbid: None,
                ..
            }) => {
                return invalid("block needs require, forbid or both");
            }
            Some(Check::UnusedSymbols {
                declare,
                corpus,
                uses,
                ..
            }) if declare.is_empty() || corpus.is_empty() || uses.is_empty() => {
                return invalid("unused_symbols needs declare, corpus and uses");
            }
            Some(Check::SetCovered {
                pattern,
                in_files,
                in_pattern,
                ..
            }) if pattern.is_empty() || in_files.is_empty() || in_pattern.is_empty() => {
                return invalid("set_covered needs pattern, in_files and in_pattern");
            }
            Some(other) => return validate_structured(other, id),
            _ => {}
        }
    }
    Ok(())
}

fn validate_structured(check: &Check, id: &str) -> RulesResult<()> {
    let invalid = |msg: &str| Err(RulesError::Invalid(id.to_string(), msg.to_string()));
    match check {
        Check::Element(element) => {
            let assertions = usize::from(element.pattern.is_some())
                + usize::from(element.distinct.is_some())
                + usize::from(element.compare.is_some())
                + usize::from(element.child_values.is_some());
            if assertions > 1 {
                return invalid(
                    "element takes at most one of pattern, distinct, compare, child_values",
                );
            }
            if element.except.is_some() && element.pattern.is_none() {
                return invalid("except needs pattern");
            }
        }
        Check::Contrast(contrast) if contrast.colors.is_empty() => {
            return invalid("contrast needs at least one colors file");
        }
        Check::CssLength { names, .. }
        | Check::CssLineHeight { names, .. }
        | Check::CssScale { names, .. }
        | Check::CssRefs { names }
            if names.is_empty() =>
        {
            return invalid("names must not be empty");
        }
        Check::CssContrast { pairs } if pairs.is_empty() => {
            return invalid("css_contrast needs at least one pair");
        }
        _ => {}
    }
    Ok(())
}

/// An id is valid when it is non-empty and only uses `a-z`, `0-9` and `-`.
/// It doubles as the rule's file name, so this also keeps it a safe path segment.
pub(crate) fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
