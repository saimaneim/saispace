use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KpiMetricDef {
    pub name: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    /// `None`: a volume to watch, neither better nor worse when it moves.
    pub higher_is_better: Option<bool>,
    pub global_only: bool,
    pub project_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KpiScopeSnapshot {
    pub scope: String,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KpiComputation {
    pub day: String,
    pub computed_at: DateTime<Utc>,
    pub global: KpiScopeSnapshot,
    pub projects: Vec<KpiScopeSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningKpis {
    pub corrections_7d: f64,
    pub stop_blocks_7d: f64,
    pub lesson_conversion_7d: Option<f64>,
    /// Lessons handed back by a trigger after a tool call (7d).
    #[serde(default)]
    pub recalls_7d: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KpiPoint {
    pub day: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KpiSeries {
    pub name: String,
    pub label: String,
    pub unit: String,
    pub higher_is_better: Option<bool>,
    pub points: Vec<KpiPoint>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KpiHistory {
    pub metrics: Vec<KpiSeries>,
}
