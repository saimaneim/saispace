//! Storage for memory triggers (see [`crate::recall`]).

use crate::db::Db;
use crate::db::memory::MEMORY_COLUMNS;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, MemoryContext, MemoryScope};
use crate::recall::{MAX_TRIGGERS, normalize_triggers, trigger_fires};
use chrono::Utc;
use uuid::Uuid;

impl Db {
    /// Replaces a memory's triggers. An empty list clears them.
    ///
    /// # Errors
    ///
    /// Returns a validation error for an invalid trigger (too generic, too
    /// short, unknown host) or when the memory does not exist.
    pub async fn set_memory_triggers<S: AsRef<str> + Sync>(
        &self,
        memory_id: Uuid,
        triggers: &[S],
    ) -> CoreResult<Vec<String>> {
        let patterns = normalize_triggers(triggers).map_err(CoreError::Validation)?;
        self.get_memory(memory_id).await?;
        let now = Utc::now();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM memory_triggers WHERE memory_id = ?1")
            .bind(memory_id)
            .execute(&mut *tx)
            .await?;
        for pattern in &patterns {
            sqlx::query(
                "INSERT INTO memory_triggers (memory_id, pattern, created_at) VALUES (?1, ?2, ?3)",
            )
            .bind(memory_id)
            .bind(pattern)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(patterns)
    }

    /// # Errors
    ///
    /// Returns an error if the triggers cannot be loaded.
    pub async fn memory_triggers(&self, memory_id: Uuid) -> CoreResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT pattern FROM memory_triggers WHERE memory_id = ?1 ORDER BY created_at, pattern",
        )
        .bind(memory_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(p,)| p).collect())
    }

    /// Live memories in `scope` with at least one trigger that fires for the
    /// normalized `haystack` or one of `hosts`, newest first. Superseded
    /// memories never fire: their triggers moved to the survivor.
    ///
    /// # Errors
    ///
    /// Returns an error if the triggers or memories cannot be loaded.
    pub async fn triggered_memories(
        &self,
        scope: MemoryScope,
        haystack: &str,
        hosts: &[&str],
        env_only: bool,
    ) -> CoreResult<Vec<Memory>> {
        let (mode, project_id) = match scope {
            MemoryScope::All => (0, None),
            MemoryScope::GlobalOnly => (1, None),
            MemoryScope::Project(id) => (2, Some(id)),
        };
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            r"
            SELECT t.memory_id, t.pattern
            FROM memory_triggers t
            JOIN memories m ON m.id = t.memory_id
            WHERE m.superseded_by IS NULL
              AND (?1 = 0
                   OR (m.project_id IS NULL AND m.project_name = '')
                   OR (?1 = 2 AND m.project_id = ?2))
            ",
        )
        .bind(mode)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        let mut ids: Vec<Uuid> = Vec::new();
        for (id, pattern) in rows {
            if env_only != pattern.starts_with("env:") || ids.contains(&id) {
                continue;
            }
            if trigger_fires(&pattern, haystack, hosts) {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "SELECT {MEMORY_COLUMNS} FROM memories WHERE id IN ({placeholders}) ORDER BY updated_at DESC"
        );
        let mut query = sqlx::query_as::<_, Memory>(&sql);
        for id in &ids {
            query = query.bind(*id);
        }
        Ok(query.fetch_all(&self.pool).await?)
    }
}

impl Db {
    /// Adds the host's lessons (`env:` triggers) to a session-start context and
    /// drops them from the other sections, so each shows once.
    ///
    /// # Errors
    ///
    /// Returns an error if the triggered memories cannot be loaded.
    pub async fn attach_environment(
        &self,
        context: &mut MemoryContext,
        scope: MemoryScope,
        hosts: &[&str],
    ) -> CoreResult<()> {
        context.hosts = hosts.iter().map(|h| (*h).to_string()).collect();
        if hosts.is_empty() {
            return Ok(());
        }
        let env = self.triggered_memories(scope, "", hosts, true).await?;
        let shown = |m: &Memory| env.iter().any(|e| e.id == m.id);
        context.preferences.retain(|m| !shown(m));
        context.pinned.retain(|m| !shown(m));
        context.memories.retain(|m| !shown(m));
        context.environment = env;
        Ok(())
    }
}

/// Gives the survivor of a consolidation the absorbed memories' triggers.
pub(crate) async fn move_triggers(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    target: Uuid,
    others: &[Uuid],
) -> CoreResult<()> {
    for &other in others {
        sqlx::query(
            r"
            INSERT OR IGNORE INTO memory_triggers (memory_id, pattern, created_at)
            SELECT ?1, pattern, created_at FROM memory_triggers WHERE memory_id = ?2
            ",
        )
        .bind(target)
        .bind(other)
        .execute(&mut **tx)
        .await?;
        sqlx::query("DELETE FROM memory_triggers WHERE memory_id = ?1")
            .bind(other)
            .execute(&mut **tx)
            .await?;
    }
    // Keep the survivor within the per-memory limit (oldest triggers win), so
    // editing it later does not fail validation.
    sqlx::query(
        r"
        DELETE FROM memory_triggers
        WHERE memory_id = ?1
          AND pattern NOT IN (
              SELECT pattern FROM memory_triggers WHERE memory_id = ?1
              ORDER BY created_at, pattern LIMIT ?2
          )
        ",
    )
    .bind(target)
    .bind(i64::try_from(MAX_TRIGGERS).unwrap_or(i64::MAX))
    .execute(&mut **tx)
    .await?;
    Ok(())
}
