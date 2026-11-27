use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineKind {
    GoalCompleted,
    Release,
    TaskCompleted,
    Commit,
    Memory,
}

/// One readable entry of a project's history: a finished goal or task, a
/// release tag, a merged change (one per PR when the repo squashes or merges
/// PRs) or a recorded decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineItem {
    pub kind: TimelineKind,
    pub at: DateTime<Utc>,
    pub title: String,
    /// Longer text shown on demand: goal criteria, commit body, tag message.
    pub detail: Option<String>,
    /// Plain label for non-technical readers ("Nuevo", "Arreglo", "Decisión").
    pub label: Option<String>,
    /// Title of the goal this entry belongs to or most likely advanced.
    pub goal: Option<String>,
    pub link: Option<String>,
    pub hash: Option<String>,
    pub pr: Option<u32>,
    pub files: Vec<String>,
    pub file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineDay {
    pub date: NaiveDate,
    pub items: Vec<TimelineItem>,
}
