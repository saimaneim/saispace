pub mod db;
pub mod error;
pub mod git_history;
pub mod inventory;
pub mod learning;
pub mod memory_render;
pub mod models;
pub mod paths;
pub mod recall;
pub mod tech;
pub mod timeline;
pub mod topic_key;
pub mod vault;

pub use db::Db;
pub use db::DuplicatePair;
pub use db::StaleReport;
pub use db::kpi::{compute_learning_kpis, kpi_catalog, metric_applies_to_scope};
pub use db::memory_graph::{KEEP_SESSIONS, short_id};
pub use error::{CoreError, CoreResult};
pub use models::*;
pub use paths::{
    default_db_path, detect_agent_name, detect_project_root, kpi_events_path_for, project_key,
    vault_key_path_for,
};
pub use topic_key::normalize_topic_key;
pub use vault::Vault;
