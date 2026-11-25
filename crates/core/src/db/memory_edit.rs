use crate::db::Db;
use crate::db::memory::{MAX_MEMORY_CONTENT_CHARS, MAX_MEMORY_TITLE_CHARS, MEMORY_COLUMNS};
use crate::db::validation::text_with_limit;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, UpdateMemoryInput};
use crate::topic_key::normalize_topic_key;
use chrono::Utc;
use uuid::Uuid;

pub(crate) fn is_unique_constraint_violation(err: &sqlx::Error) -> bool {
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

        // The existing row's project name decides which prefix counts as
        // redundant in the new topic_key, and gives us NotFound for free.
        let existing = self.get_memory(id).await?;
        let project_name = existing.project_name.clone().unwrap_or_default();

        // Empty result = no topic: COALESCE below stores '' and the unique
        // index ignores it.
        let topic_key = input
            .topic_key
            .as_deref()
            .map(|raw| normalize_topic_key(raw, &project_name));

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
