use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Technology {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub version: String,
    pub ecosystem: String,
    pub source: String,
    pub docs_url: String,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedTechnology {
    pub name: String,
    pub version: String,
    pub ecosystem: String,
    pub source: String,
    pub docs_url: String,
}
