use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum MemoryKind {
    Decision,
    Architecture,
    Bugfix,
    Pattern,
    Config,
    Discovery,
    Preference,
    SessionSummary,
    Note,
}

impl MemoryKind {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "decision" => Self::Decision,
            "architecture" => Self::Architecture,
            "bugfix" => Self::Bugfix,
            "pattern" => Self::Pattern,
            "config" => Self::Config,
            "discovery" | "learning" => Self::Discovery,
            "preference" | "user" | "feedback" => Self::Preference,
            "session_summary" => Self::SessionSummary,
            "note" | "manual" | "reference" | "project" => Self::Note,
            _ => return None,
        })
    }
}

/// Who a memory belongs to: the repo (`project`, shareable with `sai sync`)
/// or the developer (`personal`, never exported).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Project,
    Personal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Memory {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub topic_key: Option<String>,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub use_count: i64,
    /// Set when the memory was consolidated into another one.
    pub superseded_by: Option<Uuid>,
    #[serde(default)]
    #[sqlx(default)]
    pub scope: Visibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMemoryInput {
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub project_name: Option<String>,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub topic_key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateMemoryInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default)]
    pub topic_key: Option<String>,
}

/// Which memories a query may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryScope {
    /// Every memory, from every project (the desktop overview).
    All,
    /// Only cross-project memories saved with `global`.
    GlobalOnly,
    /// One project's memories plus the global ones.
    Project(Uuid),
}

impl MemoryScope {
    #[must_use]
    pub fn for_project(project_id: Option<Uuid>) -> Self {
        project_id.map_or(Self::GlobalOnly, Self::Project)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryContext {
    pub preferences: Vec<Memory>,
    pub last_session: Option<Memory>,
    pub pinned: Vec<Memory>,
    pub memories: Vec<Memory>,
    /// Likely duplicates worth merging, so the index stays small over time.
    #[serde(default)]
    pub consolidate: Vec<super::memory_graph::ConsolidateHint>,
    /// Hosts the agent runs inside (`t3code`, `opencode`…), when detected.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Lessons tied to those hosts with an `env:<host>` trigger.
    #[serde(default)]
    pub environment: Vec<Memory>,
    /// Open tasks of the project's active plan, so a new or compacted session
    /// still sees what was left to finish.
    #[serde(default)]
    pub open_tasks: Vec<OpenTask>,
    /// Still-valid temporary facts (`--until`), soonest to expire first.
    #[serde(default)]
    pub temporary: Vec<TemporaryMemory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporaryMemory {
    pub memory: Memory,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTask {
    pub id: Uuid,
    pub title: String,
    pub goal: Option<String>,
    pub status: super::task::TaskStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMemorySaves {
    pub actor_name: String,
    pub last_7d: i64,
    pub last_30d: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct MostUsedMemory {
    pub id: Uuid,
    pub title: String,
    pub project_name: Option<String>,
    pub use_count: i64,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectMemoryUsage {
    pub project_name: String,
    pub total: i64,
    pub used: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryUsageStats {
    pub saves_by_agent: Vec<AgentMemorySaves>,
    pub most_used: Vec<MostUsedMemory>,
    pub never_used: i64,
    pub total: i64,
    pub by_project: Vec<ProjectMemoryUsage>,
}
