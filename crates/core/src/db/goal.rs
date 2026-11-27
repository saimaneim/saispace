//! Goals and the two-level plan: goals (milestones) with their tasks, loose
//! open tasks, and progress accounting.

use crate::db::Db;
use crate::db::task::insert_task;
use crate::db::validation::{limited_text, text_with_limit};
use crate::error::{CoreError, CoreResult};
use crate::models::{
    CreateGoalInput, Goal, GoalProgress, GoalWithTasks, Plan, PlanCounts, PlanFilter, Task,
    TaskPriority, UpdateGoalInput,
};
use chrono::Utc;
use sqlx::sqlite::SqliteConnection;
use std::collections::HashMap;
use uuid::Uuid;

const MAX_CRITERIA_CHARS: usize = 500;

const TASK_COLUMNS: &str =
    "id, project_id, title, status, priority, goal_id, created_at, updated_at, completed_at";

/// "Done when" criteria: trimmed, empty means none, capped at 500 chars.
fn clean_criteria(value: Option<&str>) -> CoreResult<Option<String>> {
    match value {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text_with_limit(
                    "criteria",
                    trimmed,
                    MAX_CRITERIA_CHARS,
                )?))
            }
        }
        None => Ok(None),
    }
}

async fn next_goal_position(conn: &mut SqliteConnection, project_id: Uuid) -> CoreResult<i64> {
    let position = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM goals WHERE project_id = ?1",
    )
    .bind(project_id)
    .fetch_one(conn)
    .await?;
    Ok(position)
}

async fn insert_goal(
    conn: &mut SqliteConnection,
    project_id: Uuid,
    title: String,
    criteria: Option<String>,
) -> CoreResult<Goal> {
    let position = next_goal_position(conn, project_id).await?;
    let now = Utc::now();
    let goal = sqlx::query_as::<_, Goal>(
        r"
        INSERT INTO goals (id, project_id, title, criteria, status, position, created_at, updated_at, completed_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)
        RETURNING id, project_id, title, criteria, status, position, created_at, updated_at, completed_at
        ",
    )
    .bind(Uuid::now_v7())
    .bind(project_id)
    .bind(title)
    .bind(criteria)
    .bind(crate::models::GoalStatus::Active)
    .bind(position)
    .bind(now)
    .bind(None::<chrono::DateTime<Utc>>)
    .fetch_one(conn)
    .await?;

    Ok(goal)
}

#[derive(sqlx::FromRow)]
struct GoalProgressRow {
    goal_id: Uuid,
    done: i64,
    total: i64,
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if the input is invalid or the goal cannot be stored.
    pub async fn create_goal(&self, input: CreateGoalInput) -> CoreResult<Goal> {
        let title = limited_text("title", &input.title)?;
        let criteria = clean_criteria(input.criteria.as_deref())?;
        let mut tx = self.pool.begin().await?;
        let goal = insert_goal(&mut tx, input.project_id, title, criteria).await?;
        tx.commit().await?;
        Ok(goal)
    }

    /// Creates a goal and its task breakdown in one transaction.
    ///
    /// # Errors
    ///
    /// Returns an error if any input is invalid or the goal cannot be stored.
    pub async fn create_goal_with_tasks(
        &self,
        project_id: Uuid,
        title: String,
        criteria: Option<String>,
        tasks: &[String],
    ) -> CoreResult<GoalWithTasks> {
        let title = limited_text("title", &title)?;
        let criteria = clean_criteria(criteria.as_deref())?;
        let mut tx = self.pool.begin().await?;
        let goal = insert_goal(&mut tx, project_id, title, criteria).await?;
        let now = Utc::now();
        let mut created = Vec::with_capacity(tasks.len());
        for (index, raw) in tasks.iter().enumerate() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(CoreError::Validation(format!(
                    "task {} must not be empty",
                    index + 1
                )));
            }
            if trimmed.chars().count() > crate::db::validation::MAX_NAME_CHARS {
                return Err(CoreError::Validation(format!(
                    "task {} must be at most {} characters",
                    index + 1,
                    crate::db::validation::MAX_NAME_CHARS
                )));
            }
            let task = insert_task(
                &mut tx,
                project_id,
                trimmed.to_string(),
                TaskPriority::Medium,
                Some(goal.id),
                now,
            )
            .await?;
            created.push(task);
        }
        tx.commit().await?;
        Ok(GoalWithTasks {
            goal,
            done: 0,
            total: i64::try_from(created.len()).unwrap_or(0),
            tasks: created,
        })
    }

    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn get_goal(&self, id: Uuid) -> CoreResult<Option<Goal>> {
        let goal = sqlx::query_as::<_, Goal>(
            r"
            SELECT id, project_id, title, criteria, status, position, created_at, updated_at, completed_at
            FROM goals
            WHERE id = ?1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(goal)
    }

    /// # Errors
    ///
    /// Returns an error if the input is invalid, the goal does not exist, or
    /// the update fails. Completing sets `completed_at`; reactivating clears
    /// it.
    pub async fn update_goal(&self, id: Uuid, input: UpdateGoalInput) -> CoreResult<Goal> {
        let title = input
            .title
            .as_deref()
            .map(|value| limited_text("title", value))
            .transpose()?;
        // Outer None: keep as is. Inner None (empty string): clear criteria.
        let criteria = match input.criteria.as_deref() {
            Some(raw) => Some(clean_criteria(Some(raw))?),
            None => None,
        };
        let now = Utc::now();

        let updated = sqlx::query_as::<_, Goal>(
            r"
            UPDATE goals
            SET title = COALESCE(?1, title),
                criteria = CASE WHEN ?2 IS NULL THEN criteria ELSE ?2 END,
                status = COALESCE(?3, status),
                position = COALESCE(?4, position),
                updated_at = ?5,
                completed_at = CASE
                    WHEN ?3 = 'completed' THEN COALESCE(completed_at, ?5)
                    WHEN ?3 = 'active' THEN NULL
                    ELSE completed_at
                END
            WHERE id = ?6
            RETURNING id, project_id, title, criteria, status, position, created_at, updated_at, completed_at
            ",
        )
        .bind(title)
        .bind(criteria)
        .bind(input.status)
        .bind(input.position)
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        updated.ok_or(CoreError::NotFound { entity: "Goal", id })
    }

    /// # Errors
    ///
    /// Returns an error if the goal does not exist or the delete fails. Its
    /// tasks keep existing with `goal_id` NULL.
    pub async fn delete_goal(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM goals WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound { entity: "Goal", id });
        }

        Ok(())
    }

    /// Tasks with a goal (open first, then by creation) and each goal's
    /// done/total, both for one project.
    ///
    /// # Errors
    ///
    /// Returns an error if the queries fail.
    async fn goal_tasks_and_progress(
        &self,
        project_id: Uuid,
    ) -> CoreResult<(HashMap<Uuid, Vec<Task>>, HashMap<Uuid, (i64, i64)>)> {
        let tasks_sql = format!(
            r"
            SELECT {TASK_COLUMNS}
            FROM tasks
            WHERE project_id = ?1 AND goal_id IS NOT NULL
            ORDER BY CASE WHEN status IN ('todo', 'in_progress', 'blocked') THEN 0 ELSE 1 END,
                     created_at ASC, id ASC
            "
        );
        let tasks = sqlx::query_as::<_, Task>(&tasks_sql)
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;
        let progress_rows: Vec<GoalProgressRow> = sqlx::query_as(
            r"
            SELECT goal_id,
                   COUNT(*) FILTER (WHERE status = 'done') AS done,
                   COUNT(*) FILTER (WHERE status != 'cancelled') AS total
            FROM tasks
            WHERE project_id = ?1 AND goal_id IS NOT NULL
            GROUP BY goal_id
            ",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        let progress: HashMap<Uuid, (i64, i64)> = progress_rows
            .into_iter()
            .map(|row| (row.goal_id, (row.done, row.total)))
            .collect();
        let mut by_goal: HashMap<Uuid, Vec<Task>> = HashMap::new();
        for task in tasks {
            if let Some(goal_id) = task.goal_id {
                by_goal.entry(goal_id).or_default().push(task);
            }
        }
        Ok((by_goal, progress))
    }

    /// The whole plan in one call: goals ordered by position with their tasks
    /// (open first, then by creation), plus open tasks without a goal.
    /// Archived goals stay out of every filter. Done/total exclude cancelled
    /// tasks from the total.
    ///
    /// # Errors
    ///
    /// Returns an error if the queries fail.
    pub async fn get_plan(&self, project_id: Uuid, filter: PlanFilter) -> CoreResult<Plan> {
        let status_sql = match filter {
            PlanFilter::Active => "AND status = 'active'",
            PlanFilter::Completed => "AND status = 'completed'",
            PlanFilter::All => "AND status IN ('active', 'completed')",
        };
        let goals_sql = format!(
            r"
            SELECT id, project_id, title, criteria, status, position, created_at, updated_at, completed_at
            FROM goals
            WHERE project_id = ?1 {status_sql}
            ORDER BY position ASC, created_at ASC
            "
        );
        let goals = sqlx::query_as::<_, Goal>(&goals_sql)
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

        let (by_goal, progress) = self.goal_tasks_and_progress(project_id).await?;

        let loose_tasks = sqlx::query_as::<_, Task>(&format!(
            r"
            SELECT {TASK_COLUMNS}
            FROM tasks
            WHERE project_id = ?1
              AND goal_id IS NULL
              AND status IN ('todo', 'in_progress', 'blocked')
            ORDER BY created_at ASC, id ASC
            "
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        let goals_with = goals
            .into_iter()
            .map(|goal| {
                let goal_tasks = by_goal.get(&goal.id).cloned().unwrap_or_default();
                let (done, total) = progress.get(&goal.id).copied().unwrap_or((0, 0));
                GoalWithTasks {
                    goal,
                    done,
                    total,
                    tasks: goal_tasks,
                }
            })
            .collect();

        let (active, completed): (i64, i64) = sqlx::query_as(
            r"
            SELECT COUNT(*) FILTER (WHERE status = 'active'),
                   COUNT(*) FILTER (WHERE status = 'completed')
            FROM goals
            WHERE project_id = ?1
            ",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(Plan {
            goals: goals_with,
            loose_tasks,
            counts: PlanCounts {
                active,
                completed,
                all: active + completed,
            },
        })
    }

    /// Progress of a goal, or `None` when the goal does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn goal_progress(&self, goal_id: Uuid) -> CoreResult<Option<GoalProgress>> {
        let Some(goal) = self.get_goal(goal_id).await? else {
            return Ok(None);
        };
        let (done, total): (i64, i64) = sqlx::query_as(
            r"
            SELECT COUNT(*) FILTER (WHERE status = 'done'),
                   COUNT(*) FILTER (WHERE status != 'cancelled')
            FROM tasks
            WHERE goal_id = ?1
            ",
        )
        .bind(goal_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(Some(GoalProgress {
            goal_id: goal.id,
            done,
            total,
            goal_done: total > 0 && done == total,
        }))
    }

    /// Full UUID or the 8-digit hex short id printed in listings.
    ///
    /// # Errors
    ///
    /// Returns an error if nothing matches or a suffix matches several goals.
    pub async fn resolve_goal_id(&self, raw: &str) -> CoreResult<Uuid> {
        self.resolve_short_id("goals", "goal", raw).await
    }
}

/// How many open tasks a session-start context lists at most.
pub const MAX_OPEN_TASKS: usize = 8;

impl Db {
    /// Adds the active plan's unfinished tasks to a session-start context:
    /// in progress first, then blocked, then to do, in plan order.
    ///
    /// # Errors
    ///
    /// Returns an error if the plan cannot be loaded.
    pub async fn attach_open_tasks(
        &self,
        context: &mut crate::models::MemoryContext,
        project_id: Uuid,
    ) -> CoreResult<()> {
        use crate::models::{OpenTask, TaskStatus};
        let plan = self.get_plan(project_id, PlanFilter::Active).await?;
        let goal_tasks = plan
            .goals
            .iter()
            .flat_map(|g| g.tasks.iter().map(move |t| (Some(g.goal.title.clone()), t)));
        let loose = plan.loose_tasks.iter().map(|t| (None, t));
        let mut open: Vec<OpenTask> = goal_tasks
            .chain(loose)
            .filter(|(_, t)| {
                matches!(
                    t.status,
                    TaskStatus::Todo | TaskStatus::InProgress | TaskStatus::Blocked
                )
            })
            .map(|(goal, t)| OpenTask {
                id: t.id,
                title: t.title.clone(),
                goal,
                status: t.status,
            })
            .collect();
        let rank = |s: TaskStatus| match s {
            TaskStatus::InProgress => 0,
            TaskStatus::Blocked => 1,
            _ => 2,
        };
        open.sort_by_key(|t| rank(t.status));
        open.truncate(MAX_OPEN_TASKS);
        context.open_tasks = open;
        Ok(())
    }
}
