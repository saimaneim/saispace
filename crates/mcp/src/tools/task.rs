use crate::server::SaiSpaceServer;
use crate::tools::{format_error, format_success, parse_uuid};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use saispace_core::models::{
    ActionKind, CreateTaskInput, EntityKind, RecordActivityInput, Task, TaskPriority, TaskStatus,
    UpdateTaskInput,
};
use schemars::JsonSchema;
use serde::Deserialize;

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
pub struct ListTasksParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub status: Option<TaskStatusParam>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateTaskParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub title: String,
    pub priority: Option<TaskPriorityParam>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTaskParams {
    pub task_id: String,
    pub title: Option<String>,
    pub status: Option<TaskStatusParam>,
    pub priority: Option<TaskPriorityParam>,
}

#[tool_router(router = task_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "list_tasks",
        description = "List tasks for a project with optional status filter"
    )]
    async fn list_tasks(&self, Parameters(params): Parameters<ListTasksParams>) -> CallToolResult {
        let project_id = match params.project_id.as_deref() {
            Some(id_str) => match parse_uuid(id_str) {
                Ok(id) => Some(id),
                Err(err) => return format_error(&err),
            },
            None => None,
        };
        let project_opt = match self
            .resolve_project(project_id, params.path.as_deref())
            .await
        {
            Ok(p) => p,
            Err(err) => return format_error(&err),
        };
        let Some(project) = project_opt else {
            return CallToolResult::error(vec![ContentBlock::text("Project not found")]);
        };
        let tasks = match self.db.list_tasks_by_project(project.id).await {
            Ok(t) => t,
            Err(err) => return format_error(&err),
        };
        let filtered: Vec<Task> = match params.status {
            Some(status_param) => {
                let target_status: TaskStatus = status_param.into();
                tasks
                    .into_iter()
                    .filter(|t| t.status == target_status)
                    .collect()
            }
            None => tasks,
        };
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: Some(project.id),
                actor: self.actor(),
                action: ActionKind::Read,
                entity: EntityKind::Task,
                entity_id: None,
                details: None,
            })
            .await;
        format_success(&filtered)
    }

    #[tool(
        name = "create_task",
        description = "Create a new task under a project"
    )]
    async fn create_task(
        &self,
        Parameters(params): Parameters<CreateTaskParams>,
    ) -> CallToolResult {
        let project_id = match params.project_id.as_deref() {
            Some(id_str) => match parse_uuid(id_str) {
                Ok(id) => Some(id),
                Err(err) => return format_error(&err),
            },
            None => None,
        };
        let project_opt = match self
            .resolve_project(project_id, params.path.as_deref())
            .await
        {
            Ok(p) => p,
            Err(err) => return format_error(&err),
        };
        let Some(project) = project_opt else {
            return CallToolResult::error(vec![ContentBlock::text("Project not found")]);
        };
        let input = CreateTaskInput {
            project_id: project.id,
            title: params.title,
            priority: params.priority.map(Into::into),
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
        description = "Update status, priority, or title of an existing task"
    )]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> CallToolResult {
        let task_id = match parse_uuid(&params.task_id) {
            Ok(id) => id,
            Err(err) => return format_error(&err),
        };
        let input = UpdateTaskInput {
            title: params.title,
            status: params.status.map(Into::into),
            priority: params.priority.map(Into::into),
        };
        let task = match self.db.update_task(task_id, input).await {
            Ok(t) => t,
            Err(err) => return format_error(&err),
        };
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: Some(task.project_id),
                actor: self.actor(),
                action: ActionKind::Update,
                entity: EntityKind::Task,
                entity_id: Some(task.id),
                details: None,
            })
            .await;
        format_success(&task)
    }
}
