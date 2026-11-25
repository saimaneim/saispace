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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    Forbid {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_at: Option<String>,
    },
    Require {
        pattern: String,
    },
    MaxLines {
        max: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleDef {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub severity: Severity,
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
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
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return invalid("id must use lowercase letters, digits and dashes");
        }
        if self.description.trim().is_empty() {
            return invalid("description must not be empty");
        }
        if self.files.is_empty() {
            return invalid("files must contain at least one glob");
        }
        if self.check.is_none() && self.guide.as_deref().is_none_or(|g| g.trim().is_empty()) {
            return invalid("a rule needs a check, a guide, or both");
        }
        if let Some(Check::MaxLines { max: 0 }) = self.check {
            return invalid("max_lines must be greater than zero");
        }
        Ok(())
    }
}
