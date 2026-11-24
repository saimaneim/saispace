pub mod db;
pub mod error;
pub mod inventory;
pub mod memory_render;
pub mod models;
pub mod paths;
pub mod tech;
pub mod vault;

pub use db::Db;
pub use db::DuplicatePair;
pub use error::{CoreError, CoreResult};
pub use models::*;
pub use paths::{
    default_db_path, detect_agent_name, detect_project_root, project_key, vault_key_path_for,
};
pub use vault::Vault;
