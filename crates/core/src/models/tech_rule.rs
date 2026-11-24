use crate::models::Technology;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct TechRule {
    pub id: Uuid,
    pub technology: String,
    pub version_req: Option<String>,
    pub rule: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertTechRuleInput {
    pub technology: String,
    #[serde(default)]
    pub version: Option<String>,
    pub rule: String,
}

impl TechRule {
    #[must_use]
    pub fn matches(&self, stack: &[Technology]) -> bool {
        stack.iter().any(|tech| {
            tech.name.eq_ignore_ascii_case(&self.technology)
                && self
                    .version_req
                    .as_deref()
                    .is_none_or(|req| version_matches(req, &tech.version))
        })
    }
}

#[must_use]
pub fn version_matches(requirement: &str, version: &str) -> bool {
    let requirement = requirement.trim().trim_start_matches(['^', '~', '=', 'v']);
    if requirement.is_empty() || requirement == version {
        return true;
    }
    version
        .strip_prefix(requirement)
        .is_some_and(|rest| rest.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::version_matches;

    #[test]
    fn matches_on_dot_boundaries() {
        assert!(version_matches("2", "2.12.5"));
        assert!(version_matches("2.1", "2.1.9"));
        assert!(version_matches("^2", "2.0.0"));
        assert!(version_matches("2.0.0", "2.0.0"));
        assert!(!version_matches("2", "20.1.0"));
        assert!(!version_matches("2.1", "2.10.0"));
    }
}
