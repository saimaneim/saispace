use crate::db::Db;
use crate::db::search::STOPWORDS;
use crate::db::validation::text_with_limit;
use crate::error::{CoreError, CoreResult};
use crate::models::{
    Memory, MemoryContext, MemoryKind, MemoryScope, SaveMemoryInput, SearchKind, SimilarMemory,
};
use crate::topic_key::normalize_topic_key;
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub const MAX_MEMORY_TITLE_CHARS: usize = 200;
pub const MAX_MEMORY_CONTENT_CHARS: usize = 20_000;

pub(crate) const MEMORY_COLUMNS: &str = "id, project_id, NULLIF(project_name, '') AS project_name, kind, title, content, NULLIF(topic_key, '') AS topic_key, pinned, created_at, updated_at, last_used_at, use_count";

/// Share of meaningful title words two memories must have in common (Jaccard)
/// to count as possible duplicates. One shared word such as a client name
/// ("Onix", "Pujades") is not enough.
const MIN_TITLE_SIMILARITY: f64 = 0.5;

fn title_words(title: &str) -> std::collections::HashSet<String> {
    title
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(&w.as_str()))
        .collect()
}

/// Jaccard similarity of the meaningful words of two titles, from 0.0 to 1.0.
#[must_use]
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (title_words(a), title_words(b));
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = a.intersection(&b).count() as f64 / union as f64;
    ratio
}

impl Db {
    pub async fn save_memory(&self, input: SaveMemoryInput) -> CoreResult<Memory> {
        self.save_memory_at(input, Utc::now(), None).await
    }

    pub async fn save_memory_at(
        &self,
        input: SaveMemoryInput,
        updated_at: DateTime<Utc>,
        created_at: Option<DateTime<Utc>>,
    ) -> CoreResult<Memory> {
        let title = text_with_limit("title", &input.title, MAX_MEMORY_TITLE_CHARS)?;
        let content = text_with_limit("content", &input.content, MAX_MEMORY_CONTENT_CHARS)?;
        let project_name = match (input.project_id, input.project_name.as_deref()) {
            (Some(id), _) => {
                self.get_project(id)
                    .await?
                    .ok_or(CoreError::NotFound {
                        entity: "project",
                        id,
                    })?
                    .name
            }
            (None, Some(name)) => name.trim().to_string(),
            (None, None) => String::new(),
        };
        // Empty result = no topic: the upsert below only matches non-empty keys.
        let topic_key = match input.topic_key.as_deref() {
            None => String::new(),
            Some(raw) => normalize_topic_key(raw, &project_name),
        };

        let sql = format!(
            r"
            INSERT INTO memories (id, project_id, project_name, kind, title, content, topic_key, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT (project_name, topic_key) WHERE topic_key <> ''
            DO UPDATE SET kind = excluded.kind, title = excluded.title, content = excluded.content,
                          project_id = excluded.project_id, updated_at = excluded.updated_at
            RETURNING {MEMORY_COLUMNS}
            "
        );
        let memory = sqlx::query_as::<_, Memory>(&sql)
            .bind(Uuid::now_v7())
            .bind(input.project_id)
            .bind(project_name)
            .bind(input.kind)
            .bind(title)
            .bind(content)
            .bind(topic_key)
            .bind(created_at.unwrap_or(updated_at))
            .bind(updated_at)
            .fetch_one(&self.pool)
            .await?;
        Ok(memory)
    }

    pub async fn get_memory(&self, id: Uuid) -> CoreResult<Memory> {
        let sql = format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE id = ?1");
        sqlx::query_as::<_, Memory>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(CoreError::NotFound {
                entity: "memory",
                id,
            })
    }

    pub async fn recent_memories(&self, scope: MemoryScope, limit: i64) -> CoreResult<Vec<Memory>> {
        let (mode, project_id) = match scope {
            MemoryScope::All => (0, None),
            MemoryScope::GlobalOnly => (1, None),
            MemoryScope::Project(id) => (2, Some(id)),
        };
        let sql = format!(
            r"
            SELECT {MEMORY_COLUMNS}
            FROM memories
            WHERE ?1 = 0
               OR (project_id IS NULL AND project_name = '')
               OR (?1 = 2 AND project_id = ?2)
            ORDER BY updated_at DESC
            LIMIT ?3
            "
        );
        let items = sqlx::query_as::<_, Memory>(&sql)
            .bind(mode)
            .bind(project_id)
            .bind(limit.clamp(1, 200))
            .fetch_all(&self.pool)
            .await?;
        Ok(items)
    }

    /// Session-start context: every preference, pinned memories, the newest
    /// session summary (older ones are superseded) and up to `limit` others.
    pub async fn memory_context(
        &self,
        scope: MemoryScope,
        limit: i64,
    ) -> CoreResult<MemoryContext> {
        let limit = usize::try_from(limit.clamp(1, 100)).unwrap_or(1);
        let mut context = MemoryContext::default();
        for memory in self.recent_memories(scope, 200).await? {
            match memory.kind {
                MemoryKind::Preference => context.preferences.push(memory),
                _ if memory.pinned => context.pinned.push(memory),
                MemoryKind::SessionSummary => {
                    if context.last_session.is_none() {
                        context.last_session = Some(memory);
                    }
                }
                _ if context.memories.len() < limit => context.memories.push(memory),
                _ => {}
            }
        }
        Ok(context)
    }

    /// Memories in the same scope whose title shares words with `memory`'s
    /// title, so the caller can merge or delete duplicates instead of piling
    /// them up. Session summaries are history, never duplicates.
    pub async fn similar_memories(
        &self,
        memory: &Memory,
        limit: i64,
    ) -> CoreResult<Vec<SimilarMemory>> {
        if memory.kind == MemoryKind::SessionSummary {
            return Ok(Vec::new());
        }
        let scope = MemoryScope::for_project(memory.project_id);
        let hits = self
            .search_titles(&memory.title, scope, limit.saturating_add(10))
            .await?;
        let mut similar = Vec::new();
        for hit in hits {
            if hit.kind != SearchKind::Memory
                || hit.id == memory.id
                || title_similarity(&memory.title, &hit.title) < MIN_TITLE_SIMILARITY
            {
                continue;
            }
            let other = self.get_memory(hit.id).await?;
            if other.kind == MemoryKind::SessionSummary {
                continue;
            }
            similar.push(SimilarMemory {
                topic_key: other.topic_key.clone().unwrap_or_default(),
                hit,
            });
            if similar.len() >= usize::try_from(limit).unwrap_or(0) {
                break;
            }
        }
        Ok(similar)
    }

    pub async fn delete_memory(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM memories WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "memory",
                id,
            });
        }
        Ok(())
    }

    pub async fn link_memories_by_name(&self, project_id: Uuid, name: &str) -> CoreResult<u64> {
        let result = sqlx::query(
            "UPDATE memories SET project_id = ?1 WHERE project_id IS NULL AND project_name = ?2",
        )
        .bind(project_id)
        .bind(name)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::title_similarity;

    #[test]
    fn title_similarity_ignores_a_single_shared_name() {
        assert!(
            title_similarity(
                "Onix: encargos duplicados por carrera con la IA",
                "Onix: trabajador 1 de Cegid empresa 552 sobrescrito",
            ) < 0.5
        );
        assert!(
            title_similarity(
                "Stack detection, tech_rules and FTS5 search",
                "saispace UI rules and AI context files",
            ) < 0.5
        );
        assert!(
            title_similarity(
                "Database connection pooling configuration",
                "Database connection pooling settings",
            ) >= 0.5
        );
        assert!(title_similarity("Baúl de claves", "Claves del baúl cifradas") >= 0.5);
        assert!(title_similarity("", "de la") < 0.5);
    }
}
