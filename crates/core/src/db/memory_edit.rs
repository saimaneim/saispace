use crate::db::Db;
use crate::db::memory::{
    MAX_MEMORY_CONTENT_CHARS, MAX_MEMORY_TITLE_CHARS, MAX_TOPIC_KEY_CHARS, MEMORY_COLUMNS,
};
use crate::db::validation::text_with_limit;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, UpdateMemoryInput};
use chrono::Utc;
use uuid::Uuid;

fn is_unique_constraint_violation(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        if db_err.is_unique_violation() || db_err.message().contains("UNIQUE constraint failed") {
            return true;
        }
        // SQLITE_CONSTRAINT_UNIQUE; plain "19" would also match NOT NULL or FK failures.
        if let Some(code) = db_err.code() {
            return code == "2067";
        }
    }
    false
}

impl Db {
    pub async fn update_memory(&self, id: Uuid, input: UpdateMemoryInput) -> CoreResult<Memory> {
        let title = input
            .title
            .as_deref()
            .map(|value| text_with_limit("title", value, MAX_MEMORY_TITLE_CHARS))
            .transpose()?;

        let content = input
            .content
            .as_deref()
            .map(|value| text_with_limit("content", value, MAX_MEMORY_CONTENT_CHARS))
            .transpose()?;

        let topic_key = match input.topic_key.as_deref().map(str::trim) {
            None => None,
            Some("") => Some(String::new()),
            Some(key) => Some(text_with_limit("topic_key", key, MAX_TOPIC_KEY_CHARS)?),
        };

        let now = Utc::now();
        let sql = format!(
            r"
            UPDATE memories
            SET title = COALESCE(?1, title),
                content = COALESCE(?2, content),
                kind = COALESCE(?3, kind),
                topic_key = COALESCE(?4, topic_key),
                updated_at = ?5
            WHERE id = ?6
            RETURNING {MEMORY_COLUMNS}
            "
        );

        let query_result = sqlx::query_as::<_, Memory>(&sql)
            .bind(title)
            .bind(content)
            .bind(input.kind)
            .bind(topic_key.as_deref())
            .bind(now)
            .bind(id)
            .fetch_optional(&self.pool)
            .await;

        match query_result {
            Ok(Some(memory)) => Ok(memory),
            Ok(None) => Err(CoreError::NotFound {
                entity: "memory",
                id,
            }),
            Err(err) => {
                if is_unique_constraint_violation(&err) {
                    let key_str = topic_key
                        .as_deref()
                        .filter(|k| !k.is_empty())
                        .unwrap_or("indicado");
                    Err(CoreError::Validation(format!(
                        "ya existe una memoria con el tema '{key_str}' en este proyecto"
                    )))
                } else {
                    Err(CoreError::Database(err))
                }
            }
        }
    }
}
