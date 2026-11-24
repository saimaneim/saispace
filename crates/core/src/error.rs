use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{entity} not found with id {id}")]
    NotFound { entity: &'static str, id: Uuid },

    #[error("Background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("Vault error: {0}")]
    Vault(String),

    #[error("access to secret {secret} for agent {agent} is {status}")]
    SecretAccess {
        secret: String,
        agent: String,
        status: &'static str,
    },

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
