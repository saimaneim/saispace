use crate::db::Db;
use crate::db::memory::MEMORY_COLUMNS;
use crate::error::{CoreError, CoreResult};
use crate::models::Memory;
use uuid::Uuid;

impl Db {
    /// # Errors
    ///
    /// Returns an error if the memory does not exist or the update fails.
    pub async fn set_memory_pinned(&self, id: Uuid, pinned: bool) -> CoreResult<Memory> {
        let sql =
            format!("UPDATE memories SET pinned = ?1 WHERE id = ?2 RETURNING {MEMORY_COLUMNS}");
        let memory = sqlx::query_as::<_, Memory>(&sql)
            .bind(pinned)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(CoreError::NotFound {
                entity: "memory",
                id,
            })?;
        Ok(memory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MemoryKind, MemoryScope, SaveMemoryInput};

    #[tokio::test]
    async fn set_memory_pinned_flow() {
        let db = Db::connect_in_memory().await.unwrap();
        db.migrate().await.unwrap();

        let memory = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Note,
                title: "Nota 1".into(),
                content: "Contenido 1".into(),
                topic_key: Some("topic-1".into()),
            })
            .await
            .unwrap();
        assert!(!memory.pinned);

        let pinned = db.set_memory_pinned(memory.id, true).await.unwrap();
        assert!(pinned.pinned);
        assert_eq!(pinned.id, memory.id);

        let fetched = db.get_memory(memory.id).await.unwrap();
        assert!(fetched.pinned);

        let upserted = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Note,
                title: "Nota 1 actualizada".into(),
                content: "Contenido nuevo".into(),
                topic_key: Some("topic-1".into()),
            })
            .await
            .unwrap();
        assert_eq!(upserted.id, memory.id);
        assert!(upserted.pinned);

        let unpinned = db.set_memory_pinned(memory.id, false).await.unwrap();
        assert!(!unpinned.pinned);

        let err = db
            .set_memory_pinned(Uuid::now_v7(), true)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::NotFound { .. }));
    }

    #[tokio::test]
    async fn memory_context_with_pinned() {
        let db = Db::connect_in_memory().await.unwrap();
        db.migrate().await.unwrap();

        let pref = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Preference,
                title: "Pref".into(),
                content: "Siempre español".into(),
                topic_key: None,
            })
            .await
            .unwrap();
        db.set_memory_pinned(pref.id, true).await.unwrap();

        let note_normal = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Note,
                title: "Nota normal".into(),
                content: "Texto normal".into(),
                topic_key: None,
            })
            .await
            .unwrap();

        let note_pinned = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Decision,
                title: "Decisión importante".into(),
                content: "Arquitectura hexagonal".into(),
                topic_key: None,
            })
            .await
            .unwrap();
        db.set_memory_pinned(note_pinned.id, true).await.unwrap();

        let summary = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::SessionSummary,
                title: "Resumen".into(),
                content: "Se hizo todo".into(),
                topic_key: None,
            })
            .await
            .unwrap();

        let context = db.memory_context(MemoryScope::All, 10).await.unwrap();

        assert_eq!(context.preferences.len(), 1);
        assert_eq!(context.preferences[0].id, pref.id);

        assert_eq!(context.pinned.len(), 1);
        assert_eq!(context.pinned[0].id, note_pinned.id);

        assert_eq!(context.memories.len(), 1);
        assert_eq!(context.memories[0].id, note_normal.id);

        assert!(context.last_session.is_some());
        assert_eq!(context.last_session.as_ref().unwrap().id, summary.id);
    }
}
