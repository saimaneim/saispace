use crate::db::Db;
use crate::error::CoreResult;
use chrono::{DateTime, Utc};
use sqlx::{Executor, QueryBuilder, Sqlite};
use std::collections::HashSet;
use uuid::Uuid;

const TOUCH_CHUNK_SIZE: usize = 500;

async fn touch_chunk<'e, E>(executor: E, now: DateTime<Utc>, ids: &[Uuid]) -> CoreResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    let mut builder = QueryBuilder::<Sqlite>::new("UPDATE memories SET last_used_at = ");
    builder.push_bind(now);
    builder.push(", use_count = use_count + 1 WHERE id IN (");
    let mut separated = builder.separated(", ");
    for id in ids {
        separated.push_bind(*id);
    }
    separated.push_unseparated(")");
    builder.build().execute(executor).await?;
    Ok(())
}

impl Db {
    pub async fn touch_memories(&self, ids: &[Uuid]) -> CoreResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let unique: Vec<Uuid> = if ids.len() == 1 {
            ids.to_vec()
        } else {
            let mut seen = HashSet::with_capacity(ids.len());
            ids.iter().copied().filter(|id| seen.insert(*id)).collect()
        };
        let now = Utc::now();
        if unique.len() <= TOUCH_CHUNK_SIZE {
            return touch_chunk(&self.pool, now, &unique).await;
        }
        let mut tx = self.pool.begin().await?;
        for chunk in unique.chunks(TOUCH_CHUNK_SIZE) {
            touch_chunk(&mut *tx, now, chunk).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MemoryKind, SaveMemoryInput};
    use chrono::Duration;

    async fn setup() -> Db {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");
        db
    }

    #[tokio::test]
    async fn touch_increments_count_sets_last_used_at_and_keeps_updated_at() {
        let db = setup().await;
        let old = Utc::now() - Duration::hours(2);
        let saved = db
            .save_memory_at(
                SaveMemoryInput {
                    project_id: None,
                    project_name: None,
                    kind: MemoryKind::Decision,
                    title: "Uso de memoria".into(),
                    content: "Contenido".into(),
                    topic_key: None,
                },
                old,
                Some(old),
            )
            .await
            .expect("save");
        assert_eq!(saved.use_count, 0);
        assert!(saved.last_used_at.is_none());

        let before_touch = Utc::now();
        db.touch_memories(&[saved.id]).await.expect("touch 1");
        let after_first = db.get_memory(saved.id).await.expect("get 1");
        assert_eq!(after_first.use_count, 1);
        assert!(
            after_first
                .last_used_at
                .is_some_and(|ts| ts >= before_touch)
        );
        assert_eq!(after_first.updated_at, saved.updated_at);
        assert_eq!(after_first.created_at, saved.created_at);

        db.touch_memories(&[saved.id]).await.expect("touch 2");
        let after_second = db.get_memory(saved.id).await.expect("get 2");
        assert_eq!(after_second.use_count, 2);
        assert!(after_second.last_used_at >= after_first.last_used_at);
        assert_eq!(after_second.updated_at, saved.updated_at);
    }

    #[tokio::test]
    async fn touch_empty_and_unknown_ids_are_harmless() {
        let db = setup().await;
        db.touch_memories(&[]).await.expect("empty noop");

        let unknown = Uuid::now_v7();
        db.touch_memories(&[unknown]).await.expect("unknown noop");

        let saved = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Note,
                title: "Mixta".into(),
                content: "Texto".into(),
                topic_key: None,
            })
            .await
            .expect("save");

        db.touch_memories(&[unknown, saved.id, saved.id])
            .await
            .expect("mixed and duplicate ids");
        let fetched = db.get_memory(saved.id).await.expect("get");
        assert_eq!(fetched.use_count, 1);
        assert!(fetched.last_used_at.is_some());
    }

    #[tokio::test]
    async fn touch_does_not_fire_fts_update_trigger() {
        let db = setup().await;
        let saved = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Pattern,
                title: "Indice FTS intacto".into(),
                content: "Buscar palabra clave fts_usage_marker".into(),
                topic_key: None,
            })
            .await
            .expect("save");

        let rowid_before: i64 = sqlx::query_scalar(
            "SELECT rowid FROM search_index WHERE kind = 'memory' AND ref_id = ?1",
        )
        .bind(saved.id)
        .fetch_one(db.pool())
        .await
        .expect("rowid before");

        db.touch_memories(&[saved.id]).await.expect("touch");

        let rowid_after: i64 = sqlx::query_scalar(
            "SELECT rowid FROM search_index WHERE kind = 'memory' AND ref_id = ?1",
        )
        .bind(saved.id)
        .fetch_one(db.pool())
        .await
        .expect("rowid after");

        assert_eq!(rowid_before, rowid_after);
    }

    #[tokio::test]
    async fn touch_handles_batches_over_sqlite_chunk_size() {
        let db = setup().await;
        let mut ids = Vec::with_capacity(620);
        for i in 0..620 {
            let mem = db
                .save_memory(SaveMemoryInput {
                    project_id: None,
                    project_name: None,
                    kind: MemoryKind::Note,
                    title: format!("Batch {i}"),
                    content: "Contenido".into(),
                    topic_key: None,
                })
                .await
                .expect("save");
            ids.push(mem.id);
        }

        db.touch_memories(&ids).await.expect("touch large batch");
        let first = db.get_memory(ids[0]).await.expect("first");
        let last = db.get_memory(ids[619]).await.expect("last");
        assert_eq!(first.use_count, 1);
        assert_eq!(last.use_count, 1);
        assert_eq!(first.last_used_at, last.last_used_at);
    }
}
