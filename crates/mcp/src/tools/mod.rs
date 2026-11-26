pub mod memory;
pub mod project;
pub mod secret;
pub mod task;
pub mod technology;

use rmcp::model::{CallToolResult, ContentBlock};
use saispace_core::CoreError;
use serde::Serialize;
use uuid::Uuid;

#[must_use]
pub fn canonicalize_path(path: &str) -> String {
    match std::fs::canonicalize(path) {
        Ok(canonical) => canonical.to_string_lossy().into_owned(),
        Err(_) => path.to_string(),
    }
}

/// # Errors
///
/// Returns an error if the string is not a valid UUID.
pub fn parse_uuid(id: &str) -> Result<Uuid, CoreError> {
    Uuid::parse_str(id).map_err(|e| CoreError::Validation(format!("Invalid UUID '{id}': {e}")))
}

/// Compact JSON: indentation costs the agent tokens and carries nothing.
pub fn format_success<T: Serialize>(value: &T) -> CallToolResult {
    let json_text = match serde_json::to_string(value) {
        Ok(text) => text,
        Err(err) => return format_error(&CoreError::from(err)),
    };
    CallToolResult::success(vec![ContentBlock::text(json_text)])
}

#[must_use]
pub fn format_error(err: &CoreError) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(err.to_string())])
}
