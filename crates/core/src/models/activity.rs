use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Actor {
    User,
    Agent { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum ActionKind {
    Create,
    Read,
    Update,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum EntityKind {
    Project,
    Task,
    ContextNotes,
    TechRule,
    Stack,
    Search,
    Memory,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub actor: Actor,
    pub action: ActionKind,
    pub entity: EntityKind,
    pub entity_id: Option<Uuid>,
    pub details: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordActivityInput {
    pub project_id: Option<Uuid>,
    pub actor: Actor,
    pub action: ActionKind,
    pub entity: EntityKind,
    pub entity_id: Option<Uuid>,
    pub details: Option<serde_json::Value>,
}
