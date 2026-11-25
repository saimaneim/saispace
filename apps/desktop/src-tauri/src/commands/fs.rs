use crate::error::CommandError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub current_path: String,
    pub display_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<DirectoryEntry>,
}

fn get_home_dir() -> PathBuf {
    std::env::var("HOME").map_or_else(|_| PathBuf::from("/"), PathBuf::from)
}

fn to_display_path(path: &Path, home: &Path) -> String {
    if let Ok(stripped) = path.strip_prefix(home) {
        if stripped.as_os_str().is_empty() {
            "~/".to_string()
        } else {
            let s = stripped.to_string_lossy();
            format!("~/{}/", s.trim_end_matches('/'))
        }
    } else {
        let lossy = path.to_string_lossy();
        if lossy == "/" {
            "/".to_string()
        } else {
            format!("{}/", lossy.trim_end_matches('/'))
        }
    }
}

/// # Errors
///
/// Returns an error if the path is not a directory or its entries cannot be read.
#[tauri::command]
pub fn list_directory(
    path: Option<String>,
    show_hidden: Option<bool>,
) -> Result<DirectoryListing, CommandError> {
    let home = get_home_dir();
    let target_path = match path {
        Some(p) if !p.trim().is_empty() => {
            let expanded = if let Some(stripped) = p.strip_prefix("~/") {
                home.join(stripped)
            } else if p == "~" {
                home.clone()
            } else {
                PathBuf::from(p)
            };
            match std::fs::canonicalize(&expanded) {
                Ok(canon) => canon,
                Err(_) => expanded,
            }
        }
        _ => home.clone(),
    };

    if !target_path.is_dir() {
        return Err(saispace_core::CoreError::Validation(format!(
            "Path '{}' is not a directory",
            target_path.display()
        ))
        .into());
    }

    let parent_path = target_path
        .parent()
        .map(|p| p.to_string_lossy().to_string());

    let include_hidden = show_hidden.unwrap_or(false);
    let mut entries = Vec::new();

    let read_dir = std::fs::read_dir(&target_path)?;
    for entry in read_dir.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().to_string();
        if !include_hidden && file_name.starts_with('.') {
            continue;
        }

        entries.push(DirectoryEntry {
            name: file_name,
            path: entry_path.to_string_lossy().to_string(),
            is_dir: true,
        });
    }

    entries.sort_by_key(|a| a.name.to_lowercase());

    Ok(DirectoryListing {
        display_path: to_display_path(&target_path, &home),
        current_path: target_path.to_string_lossy().to_string(),
        parent_path,
        entries,
    })
}
