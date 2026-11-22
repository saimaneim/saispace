use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::models::{CreateTaskInput, Task, TaskPriority, TaskStatus, UpdateTaskInput};
use chrono::Utc;
use uuid::Uuid;

impl Db {
    pub async fn create_task(&self, input: CreateTaskInput) -> CoreResult<Task> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let priority = match input.priority {
            Some(p) => p,
            None => TaskPriority::Medium,
        };

        let task = sqlx::query_as::<_, Task>(
            r#"
            INSERT INTO tasks (id, project_id, title, status, priority, created_at, updated_at, completed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            RETURNING id, project_id, title, status, priority, created_at, updated_at, completed_at
            "#,
        )
        .bind(id)
        .bind(input.project_id)
        .bind(input.title)
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
            r#"
            SELECT id, project_id, title, status, priority, created_at, updated_at, completed_at
            FROM tasks
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(task)
    }

    pub async fn list_tasks_by_project(&self, project_id: Uuid) -> CoreResult<Vec<Task>> {
        let tasks = sqlx::query_as::<_, Task>(
            r#"
            SELECT id, project_id, title, status, priority, created_at, updated_at, completed_at
            FROM tasks
            WHERE project_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(tasks)
    }

    pub async fn update_task(&self, id: Uuid, input: UpdateTaskInput) -> CoreResult<Task> {
        let existing = match self.get_task(id).await? {
            Some(t) => t,
            None => return Err(CoreError::NotFound { entity: "Task", id }),
        };

        let now = Utc::now();
        let title = match input.title {
            Some(t) => t,
            None => existing.title,
        };
        let status = match input.status {
            Some(s) => s,
            None => existing.status,
        };
        let priority = match input.priority {
            Some(p) => p,
            None => existing.priority,
        };
        let completed_at = match (existing.status, status) {
            (_, TaskStatus::Done) if existing.completed_at.is_none() => Some(now),
            (TaskStatus::Done, s) if s != TaskStatus::Done => None,
            _ => existing.completed_at,
        };

        let updated = sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks
            SET title = ?1, status = ?2, priority = ?3, updated_at = ?4, completed_at = ?5
            WHERE id = ?6
            RETURNING id, project_id, title, status, priority, created_at, updated_at, completed_at
            "#,
        )
        .bind(title)
        .bind(status)
        .bind(priority)
        .bind(now)
        .bind(completed_at)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(updated)
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
