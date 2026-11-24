use saispace_core::{CoreError, default_db_path};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub agent_name: String,
}

impl Config {
    pub fn from_env() -> Result<Self, CoreError> {
        let agent_name =
            std::env::var("SAISPACE_AGENT_NAME").unwrap_or_else(|_| "mcp-agent".to_string());
        Ok(Self {
            db_path: default_db_path()?,
            agent_name,
        })
    }
}
