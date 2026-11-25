use crate::error::{CommandError, CommandResult};
use saispace_core::CoreError;
use saispace_core::inventory::{AiInventory, scan_ai_inventory};
use serde::Deserialize;
use std::path::PathBuf;

const SVGL_API: &str = "https://api.svgl.app";
const MAX_SVG_BYTES: usize = 256 * 1024;
const MISS_TTL: std::time::Duration = std::time::Duration::from_hours(7 * 24);

/// # Errors
///
/// Returns an error if `HOME` is not set or the inventory scan fails.
#[tauri::command]
pub async fn global_ai_inventory() -> CommandResult<AiInventory> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CoreError::Validation("HOME is not set".to_string()))?;
    let inventory = tokio::task::spawn_blocking(move || scan_ai_inventory(&home))
        .await
        .map_err(CoreError::from)?;
    Ok(inventory)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Route {
    Single(String),
    Themed { dark: String },
}

#[derive(Deserialize)]
struct SvglEntry {
    title: String,
    route: Route,
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn icon_cache_dir() -> CommandResult<PathBuf> {
    let db_path = saispace_core::default_db_path()?;
    Ok(db_path.with_file_name("icons"))
}

fn http_error(err: &reqwest::Error) -> CommandError {
    CommandError::Connection(err.to_string())
}

async fn fetch_icon(query: &str) -> CommandResult<Option<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("saispace")
        .build()
        .map_err(|e| http_error(&e))?;
    let entries: Vec<SvglEntry> = client
        .get(SVGL_API)
        .query(&[("search", query)])
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| http_error(&e))?
        .json()
        .await
        .map_err(|e| http_error(&e))?;
    let wanted = normalize(query);
    let entry = entries
        .iter()
        .find(|e| normalize(&e.title) == wanted)
        .or_else(|| {
            if entries.len() == 1 {
                entries.first()
            } else {
                None
            }
        });
    let Some(entry) = entry else {
        return Ok(None);
    };
    let url = match &entry.route {
        Route::Single(url) | Route::Themed { dark: url } => url,
    };
    if !url.starts_with("https://svgl.app/") {
        return Ok(None);
    }
    let bytes = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| http_error(&e))?
        .bytes()
        .await
        .map_err(|e| http_error(&e))?;
    if bytes.len() > MAX_SVG_BYTES {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(text.contains("<svg").then_some(text))
}

/// # Errors
///
/// Returns an error if the icon cache or the remote lookup fails.
#[tauri::command]
pub async fn tech_icon(name: String) -> CommandResult<Option<String>> {
    let key = normalize(&name);
    if key.is_empty() || key.len() > 64 {
        return Ok(None);
    }
    let dir = icon_cache_dir()?;
    let hit = dir.join(format!("{key}.svg"));
    let miss = dir.join(format!("{key}.none"));
    if let Ok(svg) = tokio::fs::read_to_string(&hit).await {
        return Ok(Some(svg));
    }
    // Misses expire so an icon added to svgl later (or a better alias)
    // is picked up without clearing the cache by hand.
    if let Ok(meta) = tokio::fs::metadata(&miss).await {
        let fresh = meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age < MISS_TTL);
        if fresh {
            return Ok(None);
        }
    }
    let icon = fetch_icon(&name).await?;
    tokio::fs::create_dir_all(&dir).await?;
    match &icon {
        Some(svg) => tokio::fs::write(&hit, svg).await?,
        None => tokio::fs::write(&miss, b"").await?,
    }
    Ok(icon)
}
