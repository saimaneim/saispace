use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::git_history::read_git_history;
use crate::models::{MemoryKind, PlanFilter, TimelineDay};
use crate::timeline::{COMMIT_LIMIT, TimelineMemory, TimelineSources, build_timeline};
use chrono::{DateTime, TimeZone, Utc};
use std::path::PathBuf;
use uuid::Uuid;

impl Db {
    /// Decisions and architecture notes of a project that are still current.
    async fn timeline_memories(&self, project_id: Uuid) -> CoreResult<Vec<TimelineMemory>> {
        let rows = sqlx::query_as::<_, (MemoryKind, String, DateTime<Utc>)>(
            r"
            SELECT kind, title, created_at
            FROM memories
            WHERE project_id = ?1
              AND kind IN ('decision', 'architecture')
              AND superseded_by IS NULL
            ",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(kind, title, created_at)| TimelineMemory {
                kind,
                title,
                created_at,
            })
            .collect())
    }

    /// The project's history grouped by calendar day in `tz`: finished goals
    /// and tasks, decisions, release tags and one entry per merged change
    /// of the project's git repository.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist or a query fails. A
    /// folder without git history is not an error.
    pub async fn project_timeline<Tz>(
        &self,
        project_id: Uuid,
        tz: &Tz,
    ) -> CoreResult<Vec<TimelineDay>>
    where
        Tz: TimeZone + Sync,
    {
        let project = self
            .get_project(project_id)
            .await?
            .ok_or(CoreError::NotFound {
                entity: "project",
                id: project_id,
            })?;
        let goals = self
            .get_plan(project_id, PlanFilter::All)
            .await?
            .goals
            .into_iter()
            .map(|item| item.goal)
            .collect();
        let tasks = self.list_tasks_by_project(project_id).await?;
        let memories = self.timeline_memories(project_id).await?;
        let path = PathBuf::from(project.path);
        let git =
            tokio::task::spawn_blocking(move || read_git_history(&path, COMMIT_LIMIT)).await?;

        Ok(build_timeline(
            &TimelineSources {
                goals,
                tasks,
                memories,
                git,
            },
            tz,
        ))
    }
}
