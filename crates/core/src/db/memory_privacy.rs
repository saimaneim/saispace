//! Privacy for memories: credentials never reach the table, personal notes
//! stay out of `sai sync`, and `purge` erases a memory with no trace left.

use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, Visibility};
use crate::secret_scrub::scrub;
use sqlx::Connection as _;
use uuid::Uuid;

impl Db {
    /// `text` without credentials (token shapes and values of vault names).
    ///
    /// # Errors
    ///
    /// Returns an error if the vault names cannot be read.
    pub async fn scrub_secrets(&self, text: &str) -> CoreResult<String> {
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM secrets")
            .fetch_all(self.pool())
            .await?;
        Ok(scrub(text, &names).0)
    }

    /// # Errors
    ///
    /// Returns an error if the memory does not exist or cannot be updated.
    pub async fn set_memory_scope(&self, id: Uuid, scope: Visibility) -> CoreResult<Memory> {
        let done = sqlx::query("UPDATE memories SET scope = ?2 WHERE id = ?1")
            .bind(id)
            .bind(scope)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "memory",
                id,
            });
        }
        self.get_memory(id).await
    }

    /// Irreversibly erases a memory, and every memory consolidated into it
    /// (they usually hold the same text and would otherwise come back as
    /// active once `superseded_by` is cleared): its row, search index entry, links,
    /// triggers and expiry (cascades), the activity entries that name it, and
    /// the freed pages (`secure_delete`, FTS merge and WAL checkpoint), so the
    /// text cannot be recovered from the file.
    ///
    /// Returns the number of memories erased and whether the WAL was fully
    /// checkpointed (`false` while another connection, such as the open app,
    /// still reads it: old pages leave the WAL at its next checkpoint).
    ///
    /// # Errors
    ///
    /// Returns an error if the memory does not exist or the purge fails.
    pub async fn purge_memory(&self, id: Uuid) -> CoreResult<PurgeReport> {
        let mut conn = self.pool().acquire().await?;
        sqlx::query("PRAGMA secure_delete = ON")
            .execute(&mut *conn)
            .await?;
        let result = async {
            let mut tx = conn.begin().await?;
            let ids: Vec<Uuid> = sqlx::query_scalar(
                r"
                WITH RECURSIVE absorbed(id) AS (
                    SELECT id FROM memories WHERE id = ?1
                    UNION
                    SELECT m.id FROM memories m JOIN absorbed a ON m.superseded_by = a.id
                )
                SELECT id FROM absorbed
                ",
            )
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
            if ids.is_empty() {
                return Err(CoreError::NotFound {
                    entity: "memory",
                    id,
                });
            }
            for gone in &ids {
                sqlx::query("DELETE FROM memories WHERE id = ?1")
                    .bind(gone)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM activity_events WHERE entity_id = ?1")
                    .bind(gone)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query("INSERT INTO search_index(search_index) VALUES ('optimize')")
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(ids.len())
        }
        .await;
        let _ = sqlx::query("PRAGMA secure_delete = OFF")
            .execute(&mut *conn)
            .await;
        let purged = result?;
        // Old page images live in the WAL until a checkpoint copies them out.
        let (busy, _, _): (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&mut *conn)
            .await?;
        Ok(PurgeReport {
            purged,
            wal_clean: busy == 0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurgeReport {
    pub purged: usize,
    pub wal_clean: bool,
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use crate::models::{MemoryKind, SaveMemoryInput, Visibility};

    fn input(title: &str, content: &str) -> SaveMemoryInput {
        SaveMemoryInput {
            project_id: None,
            project_name: None,
            kind: MemoryKind::Note,
            title: title.into(),
            content: content.into(),
            topic_key: None,
        }
    }

    #[tokio::test]
    async fn saved_credentials_are_redacted_before_indexing() {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");
        let saved = db
            .save_memory(input(
                "Despliegue",
                "Usa GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789 para subir",
            ))
            .await
            .expect("save");
        assert!(!saved.content.contains("ghp_"), "{}", saved.content);
        let leaked: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM search_index WHERE body LIKE '%ghp_%'")
                .fetch_one(db.pool())
                .await
                .expect("count");
        assert_eq!(leaked, 0);
    }

    #[tokio::test]
    async fn purge_leaves_no_row_index_or_activity() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("s.db")).await.expect("db");
        let m = db
            .save_memory(input("Algo sensible", "palabrarara única"))
            .await
            .expect("save");
        db.set_memory_scope(m.id, Visibility::Personal)
            .await
            .expect("scope");
        db.purge_memory(m.id).await.expect("purge");

        let rows: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM memories),
                    (SELECT COUNT(*) FROM search_index WHERE search_index MATCH 'palabrarara')",
        )
        .fetch_one(db.pool())
        .await
        .expect("counts");
        assert_eq!(rows, (0, 0));
        assert!(
            db.purge_memory(m.id).await.is_err(),
            "second purge is NotFound"
        );
    }

    #[tokio::test]
    async fn purge_also_erases_memories_merged_into_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("s.db")).await.expect("db");
        let a = db
            .save_memory(input("Nota A", "texto viejo"))
            .await
            .expect("a");
        let b = db
            .save_memory(input("Nota B", "texto nuevo"))
            .await
            .expect("b");
        let merged = db
            .consolidate_memories(crate::models::ConsolidateInput {
                ids: vec![b.id, a.id],
                title: "Nota".into(),
                content: "texto nuevo y viejo".into(),
                kind: None,
                topic_key: None,
            })
            .await
            .expect("merge");
        let report = db.purge_memory(merged.id).await.expect("purge");
        assert_eq!(report.purged, 2);
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
            .fetch_one(db.pool())
            .await
            .expect("count");
        assert_eq!(left, 0, "the absorbed memory must not come back");
    }
}
