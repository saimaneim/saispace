use crate::db::Db;
use crate::db::validation::limited_text;
use crate::error::{CoreError, CoreResult};
use crate::models::{CreateTaskInput, Task, TaskPriority, TaskStatus, TaskUpdate, UpdateTaskInput};
use chrono::Utc;
use sqlx::sqlite::SqliteConnection;
use uuid::Uuid;

const TASK_COLUMNS: &str =
    "id, project_id, title, status, priority, goal_id, created_at, updated_at, completed_at";

/// Shared with `db::goal` so a goal and its breakdown insert in one
/// transaction.
///
/// # Errors
///
/// Returns an error if the task cannot be stored.
pub(crate) async fn insert_task(
    conn: &mut SqliteConnection,
    project_id: Uuid,
    title: String,
    priority: TaskPriority,
    goal_id: Option<Uuid>,
    now: chrono::DateTime<Utc>,
) -> CoreResult<Task> {
    let task = sqlx::query_as::<_, Task>(&format!(
        r"
        INSERT INTO tasks (id, project_id, title, status, priority, goal_id, created_at, updated_at, completed_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)
        RETURNING {TASK_COLUMNS}
        "
    ))
    .bind(Uuid::now_v7())
    .bind(project_id)
    .bind(title)
    .bind(TaskStatus::Todo)
    .bind(priority)
    .bind(goal_id)
    .bind(now)
    .bind(None::<chrono::DateTime<Utc>>)
    .fetch_one(conn)
    .await?;

    Ok(task)
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if the input is invalid, the goal does not exist, or
    /// the task cannot be stored.
    pub async fn create_task(&self, input: CreateTaskInput) -> CoreResult<Task> {
        let title = limited_text("title", &input.title)?;
        if let Some(goal_id) = input.goal_id {
            self.get_goal(goal_id).await?.ok_or(CoreError::NotFound {
                entity: "Goal",
                id: goal_id,
            })?;
        }
        let priority = input.priority.unwrap_or(TaskPriority::Medium);
        let now = Utc::now();
        let mut conn = self.pool.acquire().await?;
        let task = insert_task(
            &mut conn,
            input.project_id,
            title,
            priority,
            input.goal_id,
            now,
        )
        .await?;

        Ok(task)
    }

    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn get_task(&self, id: Uuid) -> CoreResult<Option<Task>> {
        let task = sqlx::query_as::<_, Task>(&format!(
            r"
            SELECT {TASK_COLUMNS}
            FROM tasks
            WHERE id = ?1
            "
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(task)
    }

    /// # Errors
    ///
    /// Returns an error if the tasks cannot be loaded.
    pub async fn list_tasks_by_project(&self, project_id: Uuid) -> CoreResult<Vec<Task>> {
        let tasks = sqlx::query_as::<_, Task>(&format!(
            r"
            SELECT {TASK_COLUMNS}
            FROM tasks
            WHERE project_id = ?1
            ORDER BY created_at ASC
            "
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(tasks)
    }

    /// # Errors
    ///
    /// Returns an error if the input is invalid, the task does not exist, or
    /// the update fails.
    pub async fn update_task(&self, id: Uuid, input: UpdateTaskInput) -> CoreResult<TaskUpdate> {
        let title = input
            .title
            .as_deref()
            .map(|value| limited_text("title", value))
            .transpose()?;
        let now = Utc::now();

        let updated = sqlx::query_as::<_, Task>(&format!(
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
            RETURNING {TASK_COLUMNS}
            "
        ))
        .bind(title)
        .bind(input.status)
        .bind(input.priority)
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let task = updated.ok_or(CoreError::NotFound { entity: "Task", id })?;
        let goal_progress = match task.goal_id {
            Some(goal_id) => self.goal_progress(goal_id).await?,
            None => None,
        };

        Ok(TaskUpdate {
            task,
            goal_progress,
        })
    }

    /// # Errors
    ///
    /// Returns an error if the task does not exist or the delete fails.
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

    /// Full UUID or the 8-digit hex short id printed in listings.
    ///
    /// # Errors
    ///
    /// Returns an error if nothing matches or a suffix matches several tasks.
    pub async fn resolve_task_id(&self, raw: &str) -> CoreResult<Uuid> {
        self.resolve_short_id("tasks", "task", raw).await
    }
}
