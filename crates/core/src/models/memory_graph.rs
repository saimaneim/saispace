use super::memory::MemoryKind;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How two memories relate. Links are directed: `from` relates to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum MemoryRelation {
    /// Same subject; read one, consider the other.
    Related,
    /// `from` builds on or needs `to` (a fix that depends on a decision).
    DependsOn,
    /// `from` absorbed `to` through consolidation.
    Supersedes,
}

impl MemoryRelation {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "related" | "relates" | "see_also" => Self::Related,
            "depends_on" | "depends" | "requires" => Self::DependsOn,
            "supersedes" | "replaces" => Self::Supersedes,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Related => "related",
            Self::DependsOn => "depends_on",
            Self::Supersedes => "supersedes",
        }
    }
}

/// Whether the linked memory is the target (`out`) or the source (`in`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkDirection {
    Out,
    In,
}

/// One neighbour of a memory, small enough to list without opening it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryLink {
    pub id: Uuid,
    pub title: String,
    pub kind: MemoryKind,
    pub relation: MemoryRelation,
    pub direction: LinkDirection,
    /// True when the neighbour was consolidated away (kept for history).
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidateInput {
    pub ids: Vec<Uuid>,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default)]
    pub topic_key: Option<String>,
}

/// Two active memories that look like the same knowledge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidateHint {
    pub a: Uuid,
    pub a_title: String,
    pub b: Uuid,
    pub b_title: String,
}
