use crate::server::SaiSpaceServer;
use crate::tools::{canonicalize_path, format_error, format_success, parse_uuid};
use chrono::{DateTime, Utc};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use saispace_core::CoreError;
use saispace_core::models::{
    ActionKind, EntityKind, Memory, MemoryKind, MemoryScope, Project, RecordActivityInput,
    SaveMemoryInput, SearchHit,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveMemoryParams {
    pub title: String,
    pub content: String,
    pub kind: Option<String>,
    pub topic_key: Option<String>,
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub global: Option<bool>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryContextParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryIdParams {
    pub id: String,
}

#[derive(Debug, Serialize)]
struct ContextItem {
    id: Uuid,
    kind: MemoryKind,
    title: String,
    updated_at: DateTime<Utc>,
    preview: String,
}

impl ContextItem {
    fn new(m: &Memory, max: usize) -> Self {
        let flat = m.content.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut preview: String = flat.chars().take(max).collect();
        if flat.chars().count() > max {
            preview.push('…');
        }
        Self {
            id: m.id,
            kind: m.kind,
            title: m.title.clone(),
            updated_at: m.updated_at,
            preview,
        }
    }
}

#[derive(Debug, Serialize)]
struct ContextResponse {
    project: Option<Project>,
    preferences: Vec<ContextItem>,
    last_session: Option<ContextItem>,
    pinned: Vec<ContextItem>,
    memories: Vec<ContextItem>,
    hint: &'static str,
}

#[derive(Debug, Serialize)]
struct SaveResponse {
    memory: Memory,
    similar: Vec<SearchHit>,
    hint: Option<&'static str>,
}

impl SaiSpaceServer {
    /// Project for an explicit `project_id` or `path`, else for the server's
    /// working directory, which MCP clients set to the agent's project.
    /// `register` creates the project on first save.
    pub(crate) async fn memory_project(
        &self,
        project_id: Option<&str>,
        path: Option<&str>,
        register: bool,
    ) -> Result<Option<Project>, CallToolResult> {
        if let Some(raw) = project_id {
            let id = parse_uuid(raw).map_err(|e| format_error(&e))?;
            return match self.db.get_project(id).await {
                Ok(Some(project)) => Ok(Some(project)),
                Ok(None) => Err(CallToolResult::error(vec![ContentBlock::text(
                    "Project not found",
                )])),
                Err(err) => Err(format_error(&err)),
            };
        }
        let dir = match path {
            Some(p) => PathBuf::from(canonicalize_path(p.trim())),
            None => match std::env::current_dir() {
                Ok(dir) => dir,
                Err(_) => return Ok(None),
            },
        };
        self.db
            .project_for_path(&dir, register)
            .await
            .map_err(|e| format_error(&e))
    }
}

#[tool_router(router = memory_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "save_memory",
        description = "Persist a durable memory (decision, architecture, bugfix, pattern, config, discovery, preference, session_summary, note). Same topic_key in the same project replaces the previous memory. Scope: the current project (project_id, path, or the working directory); global=true for cross-project knowledge"
    )]
    async fn save_memory(
        &self,
        Parameters(params): Parameters<SaveMemoryParams>,
    ) -> CallToolResult {
        let kind_raw = params.kind.as_deref().unwrap_or("note");
        let Some(kind) = MemoryKind::parse(kind_raw) else {
            return format_error(&CoreError::Validation(format!(
                "unknown memory kind '{kind_raw}'"
            )));
        };
        let project_id = if params.global == Some(true) {
            None
        } else {
            match self
                .memory_project(params.project_id.as_deref(), params.path.as_deref(), true)
                .await
            {
                Ok(Some(project)) => Some(project.id),
                Ok(None) => {
                    return format_error(&CoreError::Validation(
                        "no project for this directory: pass the project path, or global=true for cross-project knowledge".to_string(),
                    ));
                }
                Err(result) => return result,
            }
        };
        let mut saved = match self
            .db
            .save_memory(SaveMemoryInput {
                project_id,
                project_name: None,
                kind,
                title: params.title,
                content: params.content,
                topic_key: params.topic_key,
            })
            .await
        {
            Ok(memory) => memory,
            Err(err) => return format_error(&err),
        };
        if let Some(pinned) = params.pinned {
            saved = match self.db.set_memory_pinned(saved.id, pinned).await {
                Ok(memory) => memory,
                Err(err) => return format_error(&err),
            };
        }
        let action = if saved.created_at == saved.updated_at {
            ActionKind::Create
        } else {
            ActionKind::Update
        };
        self.record(RecordActivityInput {
            project_id,
            actor: self.actor(),
            action,
            entity: EntityKind::Memory,
            entity_id: Some(saved.id),
            details: Some(serde_json::json!({ "title": saved.title, "kind": saved.kind })),
        })
        .await;
        let similar = self
            .db
            .similar_memories(&saved, 3)
            .await
            .unwrap_or_default();
        let hint = (!similar.is_empty()).then_some(
            "Similar memories exist: merge by saving again with the same topic_key, or delete_memory the stale one",
        );
        format_success(&SaveResponse {
            memory: saved,
            similar,
            hint,
        })
    }

    #[tool(
        name = "memory_context",
        description = "Session-start memory index for the current project (defaults to the working directory): user preferences, the last session summary and recent memories with short previews. Open one with get_memory"
    )]
    async fn memory_context(
        &self,
        Parameters(params): Parameters<MemoryContextParams>,
    ) -> CallToolResult {
        let project = match self
            .memory_project(params.project_id.as_deref(), params.path.as_deref(), false)
            .await
        {
            Ok(project) => project,
            Err(result) => return result,
        };
        let scope = MemoryScope::for_project(project.as_ref().map(|p| p.id));
        let context = match self
            .db
            .memory_context(scope, params.limit.unwrap_or(15))
            .await
        {
            Ok(context) => context,
            Err(err) => return format_error(&err),
        };
        self.touch_memories(&context.listed_ids()).await;
        format_success(&ContextResponse {
            project,
            preferences: context
                .preferences
                .iter()
                .map(|m| ContextItem::new(m, 400))
                .collect(),
            last_session: context
                .last_session
                .as_ref()
                .map(|m| ContextItem::new(m, 800)),
            pinned: context
                .pinned
                .iter()
                .map(|m| ContextItem::new(m, 400))
                .collect(),
            memories: context
                .memories
                .iter()
                .map(|m| ContextItem::new(m, 160))
                .collect(),
            hint: "Open a memory with get_memory; save decisions, root causes and pending work with save_memory (same topic_key updates it)",
        })
    }

    #[tool(name = "get_memory", description = "Full content of one memory by id")]
    async fn get_memory(&self, Parameters(params): Parameters<MemoryIdParams>) -> CallToolResult {
        let id = match parse_uuid(&params.id) {
            Ok(id) => id,
            Err(err) => return format_error(&err),
        };
        match self.db.get_memory(id).await {
            Ok(memory) => {
                self.touch_memories(&[memory.id]).await;
                format_success(&memory)
            }
            Err(err) => format_error(&err),
        }
    }

    #[tool(
        name = "delete_memory",
        description = "Delete a memory that turned out to be wrong"
    )]
    async fn delete_memory(
        &self,
        Parameters(params): Parameters<MemoryIdParams>,
    ) -> CallToolResult {
        let id = match parse_uuid(&params.id) {
            Ok(id) => id,
            Err(err) => return format_error(&err),
        };
        let memory = match self.db.get_memory(id).await {
            Ok(memory) => memory,
            Err(err) => return format_error(&err),
        };
        if let Err(err) = self.db.delete_memory(id).await {
            return format_error(&err);
        }
        self.record(RecordActivityInput {
            project_id: memory.project_id,
            actor: self.actor(),
            action: ActionKind::Delete,
            entity: EntityKind::Memory,
            entity_id: Some(id),
            details: Some(serde_json::json!({ "title": memory.title })),
        })
        .await;
        format_success(&serde_json::json!({ "deleted": id }))
    }
}
