use crate::error::{CoreError, CoreResult};
use std::path::{Path, PathBuf};

/// # Errors
///
/// Returns an error if `HOME` is not set and no explicit path is configured.
pub fn default_db_path() -> CoreResult<PathBuf> {
    if let Some(value) = std::env::var_os("SAISPACE_DB_PATH").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| CoreError::Validation("HOME is not set".to_string()))?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("saispace")
        .join("saispace.db"))
}

#[must_use]
pub fn vault_key_path_for(db_path: &Path) -> PathBuf {
    if let Some(value) = std::env::var_os("SAISPACE_VAULT_KEY_PATH").filter(|v| !v.is_empty()) {
        return PathBuf::from(value);
    }
    db_path.with_file_name("vault.key")
}

#[must_use]
pub fn kpi_events_path_for(db_path: &Path) -> PathBuf {
    db_path.with_file_name("kpi-events.jsonl")
}

#[must_use]
pub fn detect_agent_name(fallback: &str) -> String {
    let var = |key: &str| std::env::var(key).ok().filter(|v| !v.trim().is_empty());
    if let Some(name) = var("SAISPACE_AGENT_NAME") {
        return name.trim().to_string();
    }
    if var("CLAUDECODE").is_some() {
        return "claude".to_string();
    }
    if var("OPENCODE").is_some() || var("OPENCODE_BIN_PATH").is_some() {
        return "opencode".to_string();
    }
    if var("GEMINI_CLI").is_some() {
        return "gemini".to_string();
    }
    fallback.to_string()
}

/// Normalized key used to match project names written by different tools:
/// `"ONIX GESTORIA"`, `"onix-gestoria"` and `"onix_gestoria"` all map to `"onixgestoria"`.
#[must_use]
pub fn project_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The directory that identifies the project containing `path`: the nearest
/// ancestor with a `.git` entry, or `path` itself when there is none.
#[must_use]
pub fn detect_project_root(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(path)
        .to_path_buf()
}

/// Whether `root` may be registered automatically as a project. Rejects the
/// filesystem root, `home` and its ancestors, and direct children of `home`
/// (such as `~/Dev`), which usually hold many projects.
#[must_use]
pub fn is_auto_project_root(root: &Path, home: Option<&Path>) -> bool {
    if root.parent().is_none() {
        return false;
    }
    let Some(home) = home else {
        return true;
    };
    if home.starts_with(root) {
        return false;
    }
    match root.strip_prefix(home) {
        Ok(rest) => rest.components().count() >= 2,
        Err(_) => true,
    }
}
