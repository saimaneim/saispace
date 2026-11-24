use crate::db::Db;
use crate::db::validation::limited_text;
use crate::error::{CoreError, CoreResult};
use crate::models::{CreateTaskInput, Task, TaskPriority, TaskStatus, UpdateTaskInput};
use chrono::Utc;
use uuid::Uuid;

impl Db {
    pub async fn create_task(&self, input: CreateTaskInput) -> CoreResult<Task> {
        let title = limited_text("title", &input.title)?;
        let id = Uuid::now_v7();
        let now = Utc::now();
        let priority = match input.priority {
            Some(p) => p,
            None => TaskPriority::Medium,
        };

        let task = sqlx::query_as::<_, Task>(
            r"
            INSERT INTO tasks (id, project_id, title, status, priority, created_at, updated_at, completed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            RETURNING id, project_id, title, status, priority, created_at, updated_at, completed_at
            ",
        )
        .bind(id)
        .bind(input.project_id)
        .bind(title)
        .bind(TaskStatus::Todo)
        .bind(priority)
        .bind(now)
        .bind(now)
        .bind(None::<chrono::DateTime<Utc>>)
        .fetch_one(&self.pool)
        .await?;

        Ok(task)
    }

    pub async fn get_task(&self, id: Uuid) -> CoreResult<Option<Task>> {
        let task = sqlx::query_as::<_, Task>(
            r"
            SELECT id, project_id, title, status, priority, created_at, updated_at, completed_at
            FROM tasks
            WHERE id = ?1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(task)
    }

    pub async fn list_tasks_by_project(&self, project_id: Uuid) -> CoreResult<Vec<Task>> {
        let tasks = sqlx::query_as::<_, Task>(
            r"
            SELECT id, project_id, title, status, priority, created_at, updated_at, completed_at
            FROM tasks
            WHERE project_id = ?1
            ORDER BY created_at ASC
            ",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(tasks)
    }

    pub async fn update_task(&self, id: Uuid, input: UpdateTaskInput) -> CoreResult<Task> {
        let title = input
            .title
            .as_deref()
            .map(|value| limited_text("title", value))
            .transpose()?;
        let now = Utc::now();

        let updated = sqlx::query_as::<_, Task>(
            r"
            UPDATE tasks
            SET title = COALESCE(?1, title),
                status = COALESCE(?2, status),
                priority = COALESCE(?3, priority),
                updated_at = ?4,
                completed_at = CASE
                    WHEN ?2 = 'done' THEN COALESCE(completed_at, ?4)
                    WHEN ?2 IS NOT NULL THEN NULL
                    ELSE completed_at
                END
            WHERE id = ?5
            RETURNING id, project_id, title, status, priority, created_at, updated_at, completed_at
            ",
        )
        .bind(title)
        .bind(input.status)
        .bind(input.priority)
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        updated.ok_or(CoreError::NotFound { entity: "Task", id })
    }

    pub async fn delete_task(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM tasks WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound { entity: "Task", id });
        }

        Ok(())
    }
}
