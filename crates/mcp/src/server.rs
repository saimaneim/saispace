use crate::instructions::INSTRUCTIONS;
use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool_handler,
};
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

// The generated handlers return immediately, so the macro emits an `async`
// trait impl without any `.await`; pedantic flags that expansion.
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler(router = self.tool_router)]
impl ServerHandler for SaiSpaceServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("saispace", crate::VERSION))
            .with_instructions(INSTRUCTIONS)
    }
}

impl SaiSpaceServer {
    #[must_use]
    pub fn new(db: Db, agent_name: String) -> Self {
        let mut tool_router = Self::project_tool_router();
        tool_router.merge(Self::task_tool_router());
        tool_router.merge(Self::plan_tool_router());
        tool_router.merge(Self::technology_tool_router());
        tool_router.merge(Self::memory_tool_router());
        tool_router.merge(Self::secret_tool_router());
        for route in tool_router.map.values_mut() {
            route.attr.input_schema = std::sync::Arc::new(compact_schema(&route.attr.input_schema));
        }

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

    pub async fn touch_memories(&self, ids: &[Uuid]) {
        if let Err(err) = self.db.touch_memories(ids).await {
            tracing::warn!(error = %err, "failed to touch memory usage");
        }
    }

    /// # Errors
    ///
    /// Returns an error if the project does not exist or its stack cannot be synced.
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

    /// # Errors
    ///
    /// Returns an error if the project cannot be looked up.
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

/// Every session loads the tool list, so drop schema keys that cost tokens
/// and tell the model nothing: the `$schema` URL and integer `format`s.
fn compact_schema(
    schema: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("$schema");
                if map
                    .get("format")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|f| {
                        matches!(
                            f,
                            "int64" | "int32" | "uint" | "uint32" | "uint64" | "double" | "float"
                        )
                    })
                {
                    map.remove("format");
                }
                map.values_mut().for_each(strip);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut value = serde_json::Value::Object(schema.clone());
    strip(&mut value);
    match value {
        serde_json::Value::Object(map) => map,
        _ => schema.clone(),
    }
}
