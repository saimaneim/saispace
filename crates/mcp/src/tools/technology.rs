use crate::server::SaiSpaceServer;
use crate::tools::{format_error, format_success, parse_uuid};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use saispace_core::models::{
    ActionKind, EntityKind, MemoryScope, Project, RecordActivityInput, SearchKind,
    UpsertTechRuleInput,
};
use schemars::JsonSchema;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SyncProjectStackParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
}

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

fn not_found() -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text("Project not found")])
}

impl SaiSpaceServer {
    async fn project_from_ref(
        &self,
        project_id: Option<&str>,
        path: Option<&str>,
    ) -> Result<Project, CallToolResult> {
        let id = match project_id {
            Some(raw) => Some(parse_uuid(raw).map_err(|e| format_error(&e))?),
            None => None,
        };
        match self.resolve_project(id, path).await {
            Ok(Some(project)) => Ok(project),
            Ok(None) => Err(not_found()),
            Err(err) => Err(format_error(&err)),
        }
    }
}

#[tool_router(router = technology_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "sync_project_stack",
        description = "Re-detect the project's stack from its manifests (Cargo.toml, package.json, pyproject.toml, requirements.txt, go.mod) and store exact versions. Run after dependencies change"
    )]
    async fn sync_project_stack(
        &self,
        Parameters(params): Parameters<SyncProjectStackParams>,
    ) -> CallToolResult {
        let project = match self
            .project_from_ref(params.project_id.as_deref(), params.path.as_deref())
            .await
        {
            Ok(project) => project,
            Err(result) => return result,
        };
        match self.sync_stack(project.id).await {
            Ok(stack) => format_success(&stack),
            Err(err) => format_error(&err),
        }
    }

    #[tool(
        name = "upsert_tech_rule",
        description = "Create or replace a short rule anchored to a technology and an optional version prefix (e.g. technology 'tauri', version '2')"
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
        description = "Full-text search (SQLite FTS5) across memories, project context notes, task titles and tech rules. Scope: the current project (project_id, path, or the working directory) plus global entries; all=true searches every project. Falls back to matching any word"
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
                Err(result) => return result,
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
        format_success(&hits)
    }
}
