use crate::server::SaiSpaceServer;
use crate::tools::{format_error, format_success};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use saispace_core::models::{
    ActionKind, EntityKind, MemoryScope, RecordActivityInput, SearchKind, UpsertTechRuleInput,
};
use schemars::JsonSchema;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpsertTechRuleParams {
    pub technology: String,
    pub version: Option<String>,
    pub rule: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchMemoryParams {
    pub query: String,
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub limit: Option<i64>,
    /// Search every project instead of the current one plus global entries.
    pub all: Option<bool>,
}

#[tool_router(router = technology_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "upsert_tech_rule",
        description = "Create or replace a short rule for a technology and optional version prefix (e.g. tauri, 2)"
    )]
    async fn upsert_tech_rule(
        &self,
        Parameters(params): Parameters<UpsertTechRuleParams>,
    ) -> CallToolResult {
        let saved = match self
            .db
            .upsert_tech_rule(UpsertTechRuleInput {
                technology: params.technology,
                version: params.version,
                rule: params.rule,
            })
            .await
        {
            Ok(rule) => rule,
            Err(err) => return format_error(&err),
        };
        self.record(RecordActivityInput {
            project_id: None,
            actor: self.actor(),
            action: ActionKind::Update,
            entity: EntityKind::TechRule,
            entity_id: Some(saved.id),
            details: Some(serde_json::json!({
                "technology": saved.technology,
                "version": saved.version_req,
            })),
        })
        .await;
        format_success(&saved)
    }

    #[tool(
        name = "search_memory",
        description = "FTS5 search over memories, project names, tasks and tech rules: current project plus global by default, all=true for every project. Falls back to any term when all terms miss"
    )]
    async fn search_memory(
        &self,
        Parameters(params): Parameters<SearchMemoryParams>,
    ) -> CallToolResult {
        let scope = if params.all == Some(true) {
            MemoryScope::All
        } else {
            match self
                .memory_project(params.project_id.as_deref(), params.path.as_deref(), false)
                .await
            {
                Ok(project) => MemoryScope::for_project(project.map(|p| p.id)),
                Err(result) => return *result,
            }
        };
        let hits = match self
            .db
            .search_scoped(&params.query, scope, params.limit)
            .await
        {
            Ok(hits) => hits,
            Err(err) => return format_error(&err),
        };
        let memory_ids: Vec<Uuid> = hits
            .iter()
            .filter(|h| h.kind == SearchKind::Memory)
            .map(|h| h.id)
            .collect();
        self.touch_memories(&memory_ids).await;
        let scope = match scope {
            MemoryScope::Project(id) => Some(id),
            _ => None,
        };
        self.record(RecordActivityInput {
            project_id: scope,
            actor: self.actor(),
            action: ActionKind::Read,
            entity: EntityKind::Search,
            entity_id: None,
            details: Some(serde_json::json!({
                "query": params.query,
                "hits": hits.len(),
            })),
        })
        .await;
        // Memories travel with their short id; the rank and project id are
        // noise to the agent. Tasks and rules keep the full id their tools need.
        let compact: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| {
                let id = if h.kind == SearchKind::Memory {
                    saispace_core::short_id(h.id)
                } else {
                    h.id.to_string()
                };
                serde_json::json!({ "kind": h.kind, "id": id, "title": h.title, "snippet": h.snippet })
            })
            .collect();
        format_success(&compact)
    }
}
