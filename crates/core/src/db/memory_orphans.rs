use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrphanGroup {
    pub project_name: String,
    pub count: i64,
    pub last_updated: DateTime<Utc>,
}

impl Db {
    pub async fn orphan_memory_groups(&self) -> CoreResult<Vec<OrphanGroup>> {
        let groups = sqlx::query_as::<_, OrphanGroup>(
            r"
            SELECT project_name, COUNT(*) AS count, MAX(updated_at) AS last_updated
            FROM memories
            WHERE project_id IS NULL AND project_name <> ''
            GROUP BY project_name
            ORDER BY last_updated DESC
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(groups)
    }

    pub async fn link_orphans_to_project(
        &self,
        project_name: &str,
        project_id: Uuid,
    ) -> CoreResult<u64> {
        let project = self
            .get_project(project_id)
            .await?
            .ok_or(CoreError::NotFound {
                entity: "Project",
                id: project_id,
            })?;

        let result = sqlx::query(
            r"
            UPDATE OR IGNORE memories
            SET project_id = ?1, project_name = ?2
            WHERE project_id IS NULL AND project_name = ?3
            ",
        )
        .bind(project.id)
        .bind(&project.name)
        .bind(project_name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateProjectInput, MemoryKind, MemoryScope, SaveMemoryInput};

    #[tokio::test]
    async fn orphan_groups_and_linking() -> CoreResult<()> {
        let db = Db::connect_in_memory().await?;
        db.migrate().await?;

        let now = Utc::now();
        db.save_memory_at(
            SaveMemoryInput {
                project_id: None,
                project_name: Some("legacy-service".to_string()),
                kind: MemoryKind::Decision,
                title: "Legacy auth choice".to_string(),
                content: "Use HMAC tokens".to_string(),
                topic_key: None,
            },
            now,
            None,
        )
        .await?;

        db.save_memory_at(
            SaveMemoryInput {
                project_id: None,
                project_name: Some("legacy-service".to_string()),
                kind: MemoryKind::Pattern,
                title: "Legacy logging pattern".to_string(),
                content: "Structured JSON logs".to_string(),
                topic_key: None,
            },
            now + chrono::Duration::seconds(5),
            None,
        )
        .await?;

        db.save_memory(SaveMemoryInput {
            project_id: None,
            project_name: Some("onix-tool".to_string()),
            kind: MemoryKind::Note,
            title: "Onix note".to_string(),
            content: "Standalone tool".to_string(),
            topic_key: None,
        })
        .await?;

        db.save_memory(SaveMemoryInput {
            project_id: None,
            project_name: None,
            kind: MemoryKind::Preference,
            title: "Global preference".to_string(),
            content: "Tabs over spaces".to_string(),
            topic_key: None,
        })
        .await?;

        let groups = db.orphan_memory_groups().await?;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].project_name, "legacy-service");
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[1].project_name, "onix-tool");
        assert_eq!(groups[1].count, 1);

        let random_id = Uuid::now_v7();
        let not_found = db
            .link_orphans_to_project("legacy-service", random_id)
            .await;
        assert!(matches!(not_found, Err(CoreError::NotFound { .. })));

        let project = db
            .create_project(CreateProjectInput {
                name: "Target Project".to_string(),
                path: "/tmp/unrelated-target-path".to_string(),
                context_notes: None,
            })
            .await?;

        let linked = db
            .link_orphans_to_project("legacy-service", project.id)
            .await?;
        assert_eq!(linked, 2);

        let scoped = db
            .recent_memories(MemoryScope::Project(project.id), 10)
            .await?;
        let project_memories: Vec<_> = scoped
            .into_iter()
            .filter(|m| m.project_id == Some(project.id))
            .collect();
        assert_eq!(project_memories.len(), 2);
        for memory in &project_memories {
            assert_eq!(memory.project_id, Some(project.id));
            assert_eq!(memory.project_name.as_deref(), Some("Target Project"));
        }

        let remaining_groups = db.orphan_memory_groups().await?;
        assert_eq!(remaining_groups.len(), 1);
        assert_eq!(remaining_groups[0].project_name, "onix-tool");

        Ok(())
    }
}
