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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    },
    Require {
        pattern: String,
    },
    MaxLines {
        max: usize,
    },
    /// Every match of `pattern` names a path (capture group 1, or the whole
    /// match) relative to the repo root that must exist, as is or with one of
    /// the `suffixes` appended.
    RefsExist {
        pattern: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        suffixes: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        match &self.check {
            Some(Check::MaxLines { max: 0 }) => {
                return invalid("max_lines must be greater than zero");
            }
            Some(
                Check::Forbid { pattern, .. }
                | Check::Require { pattern }
                | Check::RefsExist { pattern, .. },
            ) if pattern.is_empty() => {
                return invalid("pattern must not be empty");
            }
            Some(Check::Forbid {
                stop_at,
                unless_line,
                except,
                within,
                unless_span,
                ..
            }) if empty(stop_at)
                || empty(unless_line)
                || empty(except)
                || empty(within)
                || empty(unless_span) =>
            {
                return invalid(
                    "stop_at, unless_line, except, within and unless_span must not be empty",
                );
            }
            Some(Check::Forbid {
                within: None,
                unless_span: Some(_),
                ..
            }) => {
                return invalid("unless_span needs within");
            }
            _ => {}
        }
        Ok(())
    }
}

/// An id is valid when it is non-empty and only uses `a-z`, `0-9` and `-`.
/// It doubles as the rule's file name, so this also keeps it a safe path segment.
pub(crate) fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
