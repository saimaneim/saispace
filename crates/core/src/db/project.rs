use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::models::{CreateProjectInput, Project, ProjectStatus, UpdateProjectInput};
use chrono::Utc;
use uuid::Uuid;

impl Db {
    pub async fn create_project(&self, input: CreateProjectInput) -> CoreResult<Project> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let context_notes = input.context_notes.unwrap_or_default();

        let project = sqlx::query_as::<_, Project>(
            r#"
            INSERT INTO projects (id, name, path, status, context_notes, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            RETURNING id, name, path, status, context_notes, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(input.name)
        .bind(input.path)
        .bind(ProjectStatus::Active)
        .bind(context_notes)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        Ok(project)
    }

    pub async fn get_project(&self, id: Uuid) -> CoreResult<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, path, status, context_notes, created_at, updated_at
            FROM projects
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(project)
    }

    pub async fn get_project_by_path(&self, path: &str) -> CoreResult<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, path, status, context_notes, created_at, updated_at
            FROM projects
            WHERE path = ?1
            "#,
        )
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;

        Ok(project)
    }

    pub async fn list_projects(&self) -> CoreResult<Vec<Project>> {
        let projects = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, path, status, context_notes, created_at, updated_at
            FROM projects
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(projects)
    }

    pub async fn update_project(&self, id: Uuid, input: UpdateProjectInput) -> CoreResult<Project> {
        let existing = match self.get_project(id).await? {
            Some(p) => p,
            None => {
                return Err(CoreError::NotFound {
                    entity: "Project",
                    id,
                });
            }
        };

        let name = match input.name {
            Some(n) => n,
            None => existing.name,
        };
        let path = match input.path {
            Some(p) => p,
            None => existing.path,
        };
        let status = match input.status {
            Some(s) => s,
            None => existing.status,
        };
        let context_notes = match input.context_notes {
            Some(c) => c,
            None => existing.context_notes,
        };
        let now = Utc::now();

        let updated = sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects
            SET name = ?1, path = ?2, status = ?3, context_notes = ?4, updated_at = ?5
            WHERE id = ?6
            RETURNING id, name, path, status, context_notes, created_at, updated_at
            "#,
        )
        .bind(name)
        .bind(path)
        .bind(status)
        .bind(context_notes)
        .bind(now)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(updated)
    }

    pub async fn delete_project(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM projects WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "Project",
                id,
            });
        }

        Ok(())
    }
}
