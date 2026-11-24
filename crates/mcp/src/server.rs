use rmcp::{ServerHandler, handler::server::router::tool::ToolRouter, tool_handler};
use saispace_core::models::{
    ActionKind, Actor, EntityKind, Project, RecordActivityInput, Technology,
};
use saispace_core::{CoreError, Db};
use uuid::Uuid;

#[derive(Clone)]
pub struct SaiSpaceServer {
    pub db: Db,
    pub agent_name: String,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(
    router = self.tool_router,
    name = "saispace",
    version = "0.1.0",
    instructions = "Local project and context command center for AI agents"
)]
impl ServerHandler for SaiSpaceServer {}

impl SaiSpaceServer {
    #[must_use]
    pub fn new(db: Db, agent_name: String) -> Self {
        let mut tool_router = Self::project_tool_router();
        tool_router.merge(Self::task_tool_router());
        tool_router.merge(Self::technology_tool_router());
        tool_router.merge(Self::memory_tool_router());
        tool_router.merge(Self::secret_tool_router());

        Self {
            db,
            agent_name,
            tool_router,
        }
    }

    #[must_use]
    pub fn actor(&self) -> Actor {
        Actor::Agent {
            name: self.agent_name.clone(),
        }
    }

    pub async fn record(&self, input: RecordActivityInput) {
        if let Err(err) = self.db.record_activity(input).await {
            tracing::warn!(error = %err, "failed to record activity");
        }
    }

    pub async fn sync_stack(&self, project_id: Uuid) -> Result<Vec<Technology>, CoreError> {
        let stack = self.db.sync_project_stack(project_id).await?;
        self.record(RecordActivityInput {
            project_id: Some(project_id),
            actor: self.actor(),
            action: ActionKind::Update,
            entity: EntityKind::Stack,
            entity_id: Some(project_id),
            details: Some(serde_json::json!({ "technologies": stack.len() })),
        })
        .await;
        Ok(stack)
    }

    pub async fn resolve_project(
        &self,
        project_id: Option<Uuid>,
        path: Option<&str>,
    ) -> Result<Option<Project>, CoreError> {
        if let Some(id) = project_id {
            return self.db.get_project(id).await;
        }
        if let Some(p) = path {
            let canonical = crate::tools::canonicalize_path(p);
            return self.db.get_project_by_path(&canonical).await;
        }
        Err(CoreError::Validation(
            "Either project_id or path must be provided".to_string(),
        ))
    }
}
