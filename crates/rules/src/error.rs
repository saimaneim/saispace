use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid rule file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("cannot serialize rule {id}: {source}")]
    Serialize {
        id: String,
        source: toml::ser::Error,
    },

    #[error("rule {rule}: invalid regex: {source}")]
    Regex { rule: String, source: regex::Error },

    #[error("rule {rule}: invalid glob: {source}")]
    Glob {
        rule: String,
        source: globset::Error,
    },

    #[error("rule {0}: {1}")]
    Invalid(String, String),

    #[error("rule {0} already exists (use --force to replace it)")]
    Duplicate(String),

    #[error("rule {0} not found")]
    NotFound(String),

    #[error("rule {id} is defined twice: {first} and {second}")]
    DuplicateDefinition {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
}

pub type RulesResult<T> = Result<T, RulesError>;
