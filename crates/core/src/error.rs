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

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
