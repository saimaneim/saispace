use saispace_core::CoreError;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("Database connection error: {0}")]
    Connection(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Serialize)]
pub struct ErrorPayload {
    pub message: String,
    pub kind: String,
}

impl Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let kind = match self {
            CommandError::Core(CoreError::NotFound { .. }) => "not_found",
            CommandError::Core(CoreError::Validation(_)) => "validation",
            CommandError::Core(CoreError::Database(_)) => "database",
            CommandError::Core(CoreError::Migration(_)) => "migration",
            CommandError::Core(CoreError::Json(_)) => "json",
            CommandError::Core(CoreError::Join(_)) => "internal",
            CommandError::Core(CoreError::Vault(_)) => "vault",
            CommandError::Core(CoreError::SecretAccess { .. }) => "forbidden",
            CommandError::Connection(_) => "connection",
            CommandError::Io(_) => "io",
        };

        let payload = ErrorPayload {
            message: self.to_string(),
            kind: kind.to_string(),
        };

        payload.serialize(serializer)
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
