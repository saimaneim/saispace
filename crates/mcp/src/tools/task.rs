use crate::server::SaiSpaceServer;
use crate::tools::{format_error, format_success, parse_uuid};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use saispace_core::models::{
    ActionKind, CreateTaskInput, EntityKind, RecordActivityInput, TaskPriority, TaskStatus,
    TaskUpdate, UpdateTaskInput,
};
use saispace_core::short_id;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatusParam {
    Todo,
    InProgress,
    Blocked,
    Done,
    Cancelled,
}

impl From<TaskStatusParam> for TaskStatus {
    fn from(p: TaskStatusParam) -> Self {
        match p {
            TaskStatusParam::Todo => TaskStatus::Todo,
            TaskStatusParam::InProgress => TaskStatus::InProgress,
            TaskStatusParam::Blocked => TaskStatus::Blocked,
            TaskStatusParam::Done => TaskStatus::Done,
            TaskStatusParam::Cancelled => TaskStatus::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriorityParam {
    Low,
    Medium,
    High,
    Urgent,
}

impl From<TaskPriorityParam> for TaskPriority {
    fn from(p: TaskPriorityParam) -> Self {
        match p {
            TaskPriorityParam::Low => TaskPriority::Low,
            TaskPriorityParam::Medium => TaskPriority::Medium,
            TaskPriorityParam::High => TaskPriority::High,
            TaskPriorityParam::Urgent => TaskPriority::Urgent,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateTaskParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub title: String,
    pub priority: Option<TaskPriorityParam>,
    /// Id (full or short) of the goal this task belongs to; it supplies the
    /// project when `project_id`/`path` are absent.
    pub goal_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTaskParams {
    /// Full id or the 8-digit short id shown in the plan.
    pub task_id: String,
    pub title: Option<String>,
    pub status: Option<TaskStatusParam>,
    pub priority: Option<TaskPriorityParam>,
}

/// A task update as the agent needs it: the task itself and, when it belongs
/// to a goal, that goal's new progress.
#[derive(Debug, Serialize)]
pub(crate) struct TaskUpdateView {
    id: String,
    title: String,
    status: TaskStatus,
    priority: TaskPriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal: Option<GoalProgressView>,
    /// Present when every task of the goal is done: complete it.
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

#[derive(Debug, Serialize)]
struct GoalProgressView {
    id: String,
    done: i64,
    total: i64,
    goal_done: bool,
}

#[must_use]
pub(crate) fn task_update_view(update: &TaskUpdate) -> TaskUpdateView {
    let (goal, hint) = match &update.goal_progress {
        Some(progress) => {
            let goal = GoalProgressView {
                id: short_id(progress.goal_id),
                done: progress.done,
                total: progress.total,
                goal_done: progress.goal_done,
            };
            let hint = progress.goal_done.then(|| {
                format!(
                    "every task of goal {} is done: complete it with update_goal",
                    goal.id
                )
            });
            (Some(goal), hint)
        }
        None => (None, None),
    };
    TaskUpdateView {
        id: short_id(update.task.id),
        title: update.task.title.clone(),
        status: update.task.status,
        priority: update.task.priority,
        goal,
        hint,
    }
}

#[tool_router(router = task_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "create_task",
        description = "Create a task (project_id, path, or goal_id). Only steps that matter across sessions"
    )]
    async fn create_task(
        &self,
        Parameters(params): Parameters<CreateTaskParams>,
    ) -> CallToolResult {
        let goal_id = match params.goal_id.as_deref() {
            Some(raw) => match self.db.resolve_goal_id(raw).await {
                Ok(id) => Some(id),
                Err(err) => return format_error(&err),
            },
            None => None,
        };
        let goal = match goal_id {
            Some(id) => match self.db.get_goal(id).await {
                Ok(Some(goal)) => Some(goal),
                Ok(None) => {
                    return CallToolResult::error(vec![ContentBlock::text("Goal not found")]);
                }
                Err(err) => return format_error(&err),
            },
            None => None,
        };
        let project = if params.project_id.is_none() && params.path.is_none() {
            // The goal carries the project: a task can be created from the
            // plan alone, without repeating the project id.
            match &goal {
                Some(goal) => match self.db.get_project(goal.project_id).await {
                    Ok(Some(project)) => Some(project),
                    Ok(None) => {
                        return CallToolResult::error(vec![ContentBlock::text(
                            "Project not found",
                        )]);
                    }
                    Err(err) => return format_error(&err),
                },
                None => {
                    return format_error(&saispace_core::CoreError::Validation(
                        "Either project_id, path or goal_id must be provided".to_string(),
                    ));
                }
            }
        } else {
            let project_id = match params.project_id.as_deref() {
                Some(id_str) => match parse_uuid(id_str) {
                    Ok(id) => Some(id),
                    Err(err) => return format_error(&err),
                },
                None => None,
            };
            match self
                .resolve_project(project_id, params.path.as_deref())
                .await
            {
                Ok(p) => p,
                Err(err) => return format_error(&err),
            }
        };
        let Some(project) = project else {
            return CallToolResult::error(vec![ContentBlock::text("Project not found")]);
        };
        if let Some(goal) = &goal
            && goal.project_id != project.id
        {
            return format_error(&saispace_core::CoreError::Validation(format!(
                "goal {} belongs to a different project",
                goal.id
            )));
        }
        let input = CreateTaskInput {
            project_id: project.id,
            title: params.title,
            priority: params.priority.map(Into::into),
            goal_id,
        };
        let task = match self.db.create_task(input).await {
            Ok(t) => t,
            Err(err) => return format_error(&err),
        };
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: Some(project.id),
                actor: self.actor(),
                action: ActionKind::Create,
                entity: EntityKind::Task,
                entity_id: Some(task.id),
                details: None,
            })
            .await;
        format_success(&task)
    }

    #[tool(
        name = "update_task",
        description = "Update a task's status, priority or title (full or short id); returns its goal's progress"
    )]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> CallToolResult {
        let task_id = match self.db.resolve_task_id(&params.task_id).await {
            Ok(id) => id,
            Err(err) => return format_error(&err),
        };
        let input = UpdateTaskInput {
            title: params.title,
            status: params.status.map(Into::into),
            priority: params.priority.map(Into::into),
        };
        let updated = match self.db.update_task(task_id, input).await {
            Ok(u) => u,
            Err(err) => return format_error(&err),
        };
        let task = &updated.task;
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: Some(task.project_id),
                actor: self.actor(),
                action: ActionKind::Update,
                entity: EntityKind::Task,
                entity_id: Some(task.id),
                details: (task.status == TaskStatus::Done)
                    .then(|| serde_json::json!({ "status": "done" })),
            })
            .await;
        format_success(&task_update_view(&updated))
    }
}
