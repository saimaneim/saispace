use crate::server::SaiSpaceServer;
use crate::tools::{canonicalize_path, format_error, format_success, parse_uuid};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use saispace_core::memory_render::{excerpt, rule_preview};
use saispace_core::models::{
    ActionKind, ConsolidateHint, ConsolidateInput, EntityKind, LinkDirection, Memory, MemoryKind,
    MemoryLink, MemoryRelation, MemoryScope, Project, RecordActivityInput, SaveMemoryInput,
    SimilarMemory,
};
use saispace_core::{CoreError, short_id};
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
    /// Text that brings this lesson back on its own: an error line ("exit code
    /// 144", "permission denied (publickey)") or
    /// `env:t3code` / `env:opencode` for a host. Replaces the current list.
    pub triggers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryContextParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryIdParams {
    /// Full id or the 8-digit short id shown in listings.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LinkMemoriesParams {
    pub from: String,
    pub to: String,
    /// `related` (default) or `depends_on` (`from` needs `to`).
    pub relation: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConsolidateMemoriesParams {
    /// Ids (full or short) of 2+ memories of the same project.
    pub ids: Vec<String>,
    /// Title of the merged memory.
    pub title: String,
    /// Merged content: keep every still-true fact, drop repetition.
    pub content: String,
    pub kind: Option<String>,
    pub topic_key: Option<String>,
}

/// A link as the agent sees it: who, and how it relates from this memory.
#[derive(Debug, Serialize)]
struct LinkView {
    id: String,
    title: String,
    rel: &'static str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    archived: bool,
}

impl LinkView {
    fn new(link: &MemoryLink) -> Self {
        let rel = match (link.relation, link.direction) {
            (MemoryRelation::Related, _) => "related",
            (MemoryRelation::DependsOn, LinkDirection::Out) => "depends_on",
            (MemoryRelation::DependsOn, LinkDirection::In) => "needed_by",
            (MemoryRelation::Supersedes, LinkDirection::Out) => "absorbed",
            (MemoryRelation::Supersedes, LinkDirection::In) => "absorbed_by",
        };
        Self {
            id: short_id(link.id),
            title: link.title.clone(),
            rel,
            archived: link.archived,
        }
    }
}

/// One memory without the bookkeeping fields (use counts, raw timestamps,
/// empty values) that cost tokens and do not help the agent.
#[derive(Debug, Serialize)]
struct MemoryView {
    id: String,
    kind: MemoryKind,
    title: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    topic_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    updated: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
    /// Set when this memory was consolidated: read the survivor instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    archived_into: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    links: Vec<LinkView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    triggers: Vec<String>,
}

impl MemoryView {
    fn new(m: &Memory, links: &[MemoryLink]) -> Self {
        Self {
            id: short_id(m.id),
            kind: m.kind,
            title: m.title.clone(),
            content: m.content.clone(),
            topic_key: m.topic_key.clone(),
            project: m.project_name.clone(),
            updated: m.updated_at.format("%Y-%m-%d").to_string(),
            pinned: m.pinned,
            archived_into: m.superseded_by.map(short_id),
            links: links.iter().map(LinkView::new).collect(),
            triggers: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct BriefMemory {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    topic_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct ContextItem {
    id: String,
    kind: MemoryKind,
    title: String,
    updated: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    preview: String,
}

impl ContextItem {
    /// `max` 0 lists the title only: the agent opens what it needs by id.
    fn new(m: &Memory, max: usize) -> Self {
        let preview = match (max, m.kind) {
            (0, _) => String::new(),
            (_, MemoryKind::Preference) => rule_preview(&m.content, max),
            _ => excerpt(&m.content, max),
        };
        Self {
            id: short_id(m.id),
            kind: m.kind,
            title: m.title.clone(),
            updated: m.updated_at.format("%Y-%m-%d").to_string(),
            preview,
        }
    }
}

#[derive(Debug, Serialize)]
struct ProjectBrief {
    id: Uuid,
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct PairView {
    a: String,
    a_title: String,
    b: String,
    b_title: String,
}

impl PairView {
    fn new(hint: &ConsolidateHint) -> Self {
        Self {
            a: short_id(hint.a),
            a_title: hint.a_title.clone(),
            b: short_id(hint.b),
            b_title: hint.b_title.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ContextResponse {
    project: Option<ProjectBrief>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    preferences: Vec<ContextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_session: Option<ContextItem>,
    /// Lessons for the host the agent runs in (T3 Code, `OpenCode`…).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    environment: Vec<ContextItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pinned: Vec<ContextItem>,
    memories: Vec<ContextItem>,
    /// Look-alike pairs: merge them with `consolidate_memories` if they are one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    consolidate: Vec<PairView>,
}

#[derive(Debug, Serialize)]
struct SaveResponse {
    id: String,
    action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    topic_key: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    triggers: Vec<String>,
    /// Memories this one now links to (mentioned by id or `topic_key`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    linked: Vec<BriefMemory>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    similar: Vec<BriefMemory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

/// When the agent saved without a `topic_key` and a similar memory already
/// carries one, the cheapest fix is to save again reusing it.
fn save_hint(saved: &Memory, similar: &[SimilarMemory]) -> Option<String> {
    let generic = (!similar.is_empty()).then(|| {
        "Similar memories exist: if they are the same knowledge, merge them with consolidate_memories".to_string()
    });
    if saved
        .topic_key
        .as_deref()
        .is_some_and(|key| !key.is_empty())
    {
        return generic;
    }
    similar
        .iter()
        // Only same-scope topics update in place; another scope's key would
        // create a new row.
        .find(|s| !s.topic_key.is_empty() && s.hit.project_id == saved.project_id)
        .map(|s| {
            format!(
                "Similar memory has topic_key \"{}\": save with that topic_key to update it instead of duplicating",
                s.topic_key
            )
        })
        .or(generic)
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
    ) -> Result<Option<Project>, Box<CallToolResult>> {
        if let Some(raw) = project_id {
            let id = parse_uuid(raw).map_err(|e| Box::new(format_error(&e)))?;
            return match self.db.get_project(id).await {
                Ok(Some(project)) => Ok(Some(project)),
                Ok(None) => Err(Box::new(CallToolResult::error(vec![ContentBlock::text(
                    "Project not found",
                )]))),
                Err(err) => Err(Box::new(format_error(&err))),
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
            .map_err(|e| Box::new(format_error(&e)))
    }
}

#[tool_router(router = memory_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "save_memory",
        description = "Save durable knowledge as What/Why/Where/Learned. kind: decision, architecture, bugfix, pattern, config, discovery, preference, session_summary, note. User correction -> preference; own mistake -> bugfix with its cause and triggers (exact error line, or env:<host>). Same topic_key in the same scope updates. global=true for cross-project knowledge"
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
        if let Some(list) = &params.triggers
            && let Err(msg) = saispace_core::recall::normalize_triggers(list)
        {
            return format_error(&CoreError::Validation(msg));
        }
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
                Err(result) => return *result,
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
        let triggers = match &params.triggers {
            Some(list) => match self.db.set_memory_triggers(saved.id, list).await {
                Ok(set) => set,
                Err(err) => return format_error(&err),
            },
            None => Vec::new(),
        };
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
        let hint = save_hint(&saved, &similar);
        let linked = self
            .db
            .memory_links(saved.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|l| l.direction == LinkDirection::Out && l.relation == MemoryRelation::Related)
            .map(|l| BriefMemory {
                id: short_id(l.id),
                title: l.title,
                topic_key: None,
            })
            .collect();
        // The agent just wrote the content: echoing it back would only
        // spend its tokens again.
        format_success(&SaveResponse {
            id: short_id(saved.id),
            action: if action == ActionKind::Create {
                "created"
            } else {
                "updated"
            },
            topic_key: saved.topic_key.clone(),
            pinned: saved.pinned,
            triggers,
            linked,
            similar: similar
                .iter()
                .map(|s| BriefMemory {
                    id: short_id(s.hit.id),
                    title: s.hit.title.clone(),
                    topic_key: (!s.topic_key.is_empty()).then(|| s.topic_key.clone()),
                })
                .collect(),
            hint,
        })
    }

    #[tool(
        name = "memory_context",
        description = "Session-start memory index (project_id, path or working directory): preferences, host lessons, pinned, latest session summary, recent titles, duplicate pairs. Open items with get_memory"
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
            Err(result) => return *result,
        };
        let scope = MemoryScope::for_project(project.as_ref().map(|p| p.id));
        let mut context = match self
            .db
            .memory_context(scope, params.limit.unwrap_or(15))
            .await
        {
            Ok(context) => context,
            Err(err) => return format_error(&err),
        };
        let hosts = saispace_core::recall::detect_hosts(|key| std::env::var(key).ok());
        if let Err(err) = self
            .db
            .attach_environment(&mut context, scope, &hosts)
            .await
        {
            return format_error(&err);
        }
        format_success(&ContextResponse {
            project: project.map(|p| ProjectBrief {
                id: p.id,
                name: p.name,
                path: p.path,
            }),
            preferences: context
                .preferences
                .iter()
                .map(|m| ContextItem::new(m, 400))
                .collect(),
            last_session: context
                .last_session
                .as_ref()
                .map(|m| ContextItem::new(m, 800)),
            environment: context
                .environment
                .iter()
                .map(|m| ContextItem::new(m, 400))
                .collect(),
            pinned: context
                .pinned
                .iter()
                .map(|m| ContextItem::new(m, 400))
                .collect(),
            memories: context
                .memories
                .iter()
                .map(|m| ContextItem::new(m, 0))
                .collect(),
            consolidate: context.consolidate.iter().map(PairView::new).collect(),
        })
    }

    #[tool(
        name = "get_memory",
        description = "Full memory by id (full or 8-digit short id) with its links and triggers: follow links instead of searching again"
    )]
    async fn get_memory(&self, Parameters(params): Parameters<MemoryIdParams>) -> CallToolResult {
        let id = match self.db.resolve_memory_id(&params.id).await {
            Ok(id) => id,
            Err(err) => return format_error(&err),
        };
        let memory = match self.db.get_memory(id).await {
            Ok(memory) => memory,
            Err(err) => return format_error(&err),
        };
        let links = self.db.memory_links(memory.id).await.unwrap_or_default();
        self.touch_memories(&[memory.id]).await;
        let mut view = MemoryView::new(&memory, &links);
        view.triggers = self.db.memory_triggers(memory.id).await.unwrap_or_default();
        format_success(&view)
    }

    #[tool(
        name = "link_memories",
        description = "Link two memories: related (default) or depends_on (from needs to). Saving already links ids and topic_keys mentioned in the content"
    )]
    async fn link_memories(
        &self,
        Parameters(params): Parameters<LinkMemoriesParams>,
    ) -> CallToolResult {
        let relation = match params.relation.as_deref().map(MemoryRelation::parse) {
            None => MemoryRelation::Related,
            Some(Some(r)) if r != MemoryRelation::Supersedes => r,
            Some(_) => {
                return format_error(&CoreError::Validation(
                    "relation must be related or depends_on (consolidate_memories records supersedes)"
                        .to_string(),
                ));
            }
        };
        let (from, to) = match (
            self.db.resolve_memory_id(&params.from).await,
            self.db.resolve_memory_id(&params.to).await,
        ) {
            (Ok(from), Ok(to)) => (from, to),
            (Err(err), _) | (_, Err(err)) => return format_error(&err),
        };
        match self.db.link_memories(from, to, relation).await {
            Ok(added) => format_success(&serde_json::json!({
                "from": short_id(from),
                "to": short_id(to),
                "relation": relation.as_str(),
                "added": added,
            })),
            Err(err) => format_error(&err),
        }
    }

    #[tool(
        name = "consolidate_memories",
        description = "Merge 2+ memories of one project (or all global) that hold the same knowledge: the survivor gets your merged title/content, their links and triggers; the rest are archived (still readable by id)"
    )]
    async fn consolidate_memories(
        &self,
        Parameters(params): Parameters<ConsolidateMemoriesParams>,
    ) -> CallToolResult {
        let kind = match params.kind.as_deref() {
            None => None,
            Some(raw) => match MemoryKind::parse(raw) {
                Some(kind) => Some(kind),
                None => {
                    return format_error(&CoreError::Validation(format!(
                        "unknown memory kind '{raw}'"
                    )));
                }
            },
        };
        let mut ids = Vec::with_capacity(params.ids.len());
        for raw in &params.ids {
            match self.db.resolve_memory_id(raw).await {
                Ok(id) => ids.push(id),
                Err(err) => return format_error(&err),
            }
        }
        let merged = match self
            .db
            .consolidate_memories(ConsolidateInput {
                ids: ids.clone(),
                title: params.title,
                content: params.content,
                kind,
                topic_key: params.topic_key,
            })
            .await
        {
            Ok(memory) => memory,
            Err(err) => return format_error(&err),
        };
        self.record(RecordActivityInput {
            project_id: merged.project_id,
            actor: self.actor(),
            action: ActionKind::Update,
            entity: EntityKind::Memory,
            entity_id: Some(merged.id),
            details: Some(serde_json::json!({ "title": merged.title, "consolidated": ids.len() })),
        })
        .await;
        format_success(&serde_json::json!({
            "id": short_id(merged.id),
            "topic_key": merged.topic_key,
            "archived": ids.iter().filter(|id| **id != merged.id).map(|id| short_id(*id)).collect::<Vec<_>>(),
        }))
    }

    #[tool(
        name = "delete_memory",
        description = "Delete a memory that turned out to be wrong"
    )]
    async fn delete_memory(
        &self,
        Parameters(params): Parameters<MemoryIdParams>,
    ) -> CallToolResult {
        let id = match self.db.resolve_memory_id(&params.id).await {
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
        format_success(&serde_json::json!({ "deleted": short_id(id) }))
    }
}
