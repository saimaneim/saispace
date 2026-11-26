use crate::server::SaiSpaceServer;
use crate::tools::{canonicalize_path, format_error, format_success, parse_uuid};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use saispace_core::models::{
    ActionKind, CreateProjectInput, EntityKind, Project, RecordActivityInput, Task, TaskStatus,
    TechRule, Technology,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListProjectsParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegisterProjectParams {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetProjectContextParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectContextResponse {
    pub project: Project,
    pub open_tasks: Vec<Task>,
    pub stack: Vec<Technology>,
    pub tech_rules: Vec<TechRule>,
}

#[derive(Debug, Serialize)]
pub struct RegisterProjectResponse {
    #[serde(flatten)]
    pub project: Project,
    pub stack: Vec<Technology>,
    pub stack_error: Option<String>,
}

#[tool_router(router = project_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(name = "list_projects", description = "List all projects in SaiSpace")]
    async fn list_projects(&self, _params: Parameters<ListProjectsParams>) -> CallToolResult {
        let projects = match self.db.list_projects().await {
            Ok(p) => p,
            Err(err) => return format_error(&err),
        };
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: None,
                actor: self.actor(),
                action: ActionKind::Read,
                entity: EntityKind::Project,
                entity_id: None,
                details: None,
            })
            .await;
        format_success(&projects)
    }

    #[tool(
        name = "register_project",
        description = "Register a local project directory into SaiSpace and detect its stack (Cargo.toml, package.json and other manifests)"
    )]
    async fn register_project(
        &self,
        Parameters(params): Parameters<RegisterProjectParams>,
    ) -> CallToolResult {
        let canonical_path = canonicalize_path(params.path.trim());
        let existing = match self.db.get_project_by_path(&canonical_path).await {
            Ok(existing) => existing,
            Err(err) => return format_error(&err),
        };
        let (project, created) = if let Some(project) = existing {
            (project, false)
        } else {
            let input = CreateProjectInput {
                name: params.name,
                path: canonical_path,
            };
            match self.db.create_project(input).await {
                Ok(project) => (project, true),
                Err(err) => return format_error(&err),
            }
        };
        if created {
            self.record(RecordActivityInput {
                project_id: Some(project.id),
                actor: self.actor(),
                action: ActionKind::Create,
                entity: EntityKind::Project,
                entity_id: Some(project.id),
                details: None,
            })
            .await;
        }
        let (stack, stack_error) = match self.sync_stack(project.id).await {
            Ok(stack) => (stack, None),
            Err(err) => (Vec::new(), Some(err.to_string())),
        };
        format_success(&RegisterProjectResponse {
            project,
            stack,
            stack_error,
        })
    }

    #[tool(
        name = "get_project_context",
        description = "Get project details, open tasks, the detected stack and the tech rules matching that stack, by project_id or path"
    )]
    async fn get_project_context(
        &self,
        Parameters(params): Parameters<GetProjectContextParams>,
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
        let tasks = match self.db.list_tasks_by_project(project.id).await {
            Ok(t) => t,
            Err(err) => return format_error(&err),
        };
        let open_tasks = tasks
            .into_iter()
            .filter(|t| t.status != TaskStatus::Done && t.status != TaskStatus::Cancelled)
            .collect();
        let stack = match self.db.list_technologies(project.id).await {
            Ok(stack) => stack,
            Err(err) => return format_error(&err),
        };
        let tech_rules = match self.db.tech_rules_for_project(project.id).await {
            Ok(rules) => rules,
            Err(err) => return format_error(&err),
        };
        self.record(RecordActivityInput {
            project_id: Some(project.id),
            actor: self.actor(),
            action: ActionKind::Read,
            entity: EntityKind::Project,
            entity_id: Some(project.id),
            details: None,
        })
        .await;
        let response = ProjectContextResponse {
            project,
            open_tasks,
            stack,
            tech_rules,
        };
        format_success(&response)
    }
}
