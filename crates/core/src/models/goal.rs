use crate::models::Task;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Completed,
    Archived,
}

/// Which goals a plan query returns. Archived goals stay out of every filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum PlanFilter {
    Active,
    Completed,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Goal {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    /// Short "done when" description.
    pub criteria: Option<String>,
    pub status: GoalStatus,
    pub position: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateGoalInput {
    pub project_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub criteria: Option<String>,
    #[serde(default)]
    pub tasks: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateGoalInput {
    #[serde(default)]
    pub title: Option<String>,
    /// Empty string clears the criteria.
    #[serde(default)]
    pub criteria: Option<String>,
    #[serde(default)]
    pub status: Option<GoalStatus>,
    #[serde(default)]
    pub position: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalWithTasks {
    pub goal: Goal,
    pub done: i64,
    pub total: i64,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanCounts {
    pub active: i64,
    pub completed: i64,
    pub all: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub goals: Vec<GoalWithTasks>,
    pub loose_tasks: Vec<Task>,
    pub counts: PlanCounts,
}

/// A goal's progress after one of its tasks changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalProgress {
    pub goal_id: Uuid,
    pub done: i64,
    pub total: i64,
    /// Every non-cancelled task of the goal is done.
    pub goal_done: bool,
}
