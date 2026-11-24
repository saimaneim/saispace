use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum SearchKind {
    Project,
    Task,
    TechRule,
    Memory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SearchHit {
    pub kind: SearchKind,
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub snippet: String,
    pub rank: f64,
}
