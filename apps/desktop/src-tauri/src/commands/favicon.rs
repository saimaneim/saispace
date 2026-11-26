use crate::error::CommandResult;
use base64::Engine as _;
use saispace_core::Db;
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

const MAX_ICON_BYTES: u64 = 512 * 1024;
const MAX_DEPTH: usize = 3;
const MAX_DIRS: usize = 400;
const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "vendor",
    "__pycache__",
];

/// Lower is better. `None` means the file is not a project icon.
fn rank(relative: &Path) -> Option<u8> {
    let name = relative.file_name()?.to_str()?.to_ascii_lowercase();
    let (stem, ext) = name.rsplit_once('.')?;
    if !matches!(ext, "svg" | "ico" | "png" | "jpg" | "jpeg" | "webp") {
        return None;
    }
    let path = relative.to_string_lossy().replace('\\', "/");
    let base = match stem {
        "favicon" => 0,
        _ if stem.starts_with("favicon") => 1,
        "icon" | "apple-icon" | "apple-touch-icon" => 2,
        _ if path.ends_with("src-tauri/icons/icon.png") => 2,
        _ if stem.starts_with("logo") || stem.ends_with("-logo") || stem.ends_with("_logo") => 4,
        _ => return None,
    };
    // Vector first: it stays sharp at 16px; a jpg logo is the last resort.
    let format = match ext {
        "svg" => 0,
        "ico" | "png" | "webp" => 1,
        _ => 2,
    };
    Some(base * 3 + format)
}

fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    visited: &mut usize,
    best: &mut Vec<(u8, usize, PathBuf)>,
) {
    if *visited >= MAX_DIRS {
        return;
    }
    *visited += 1;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_dir() {
            if !name.starts_with('.') && !IGNORED_DIRS.contains(&name.as_ref()) {
                subdirs.push(path);
            }
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if let Some(score) = rank(relative) {
            let small = entry
                .metadata()
                .is_ok_and(|m| m.len() > 0 && m.len() <= MAX_ICON_BYTES);
            if small {
                best.push((score, depth, path));
            }
        }
    }
    if depth < MAX_DEPTH {
        subdirs.sort();
        for sub in subdirs {
            walk(root, &sub, depth + 1, visited, best);
        }
    }
}

fn find_icon(root: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    let mut visited = 0;
    walk(root, root, 0, &mut visited, &mut found);
    found
        .into_iter()
        .min_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)))
        .map(|(_, _, path)| path)
}

fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

fn icon_data_url(root: &Path) -> Option<String> {
    let path = find_icon(root)?;
    let bytes = std::fs::read(&path).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{};base64,{encoded}", mime(&path)))
}

/// The project's favicon or logo as a data URL, rendered through `<img>`
/// (so an SVG cannot run scripts). `None` when the project has none.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded.
#[tauri::command]
pub async fn project_favicon(id: Uuid, db: State<'_, Db>) -> CommandResult<Option<String>> {
    let Some(project) = db.get_project(id).await? else {
        return Ok(None);
    };
    let root = PathBuf::from(project.path);
    let icon = tokio::task::spawn_blocking(move || icon_data_url(&root))
        .await
        .ok()
        .flatten();
    Ok(icon)
}

#[cfg(test)]
mod tests {
    use super::{find_icon, rank};
    use std::path::Path;

    #[test]
    fn ranks_favicon_over_logo_and_svg_over_png() {
        let favicon_svg = rank(Path::new("public/favicon.svg"));
        let favicon_png = rank(Path::new("public/favicon.png"));
        let logo = rank(Path::new("public/logo.png"));
        assert!(favicon_svg < favicon_png);
        assert!(favicon_png < logo);
        assert_eq!(rank(Path::new("src/main.ts")), None);
        assert_eq!(rank(Path::new("docs/catalog.png")), None);
    }

    #[test]
    fn finds_nested_favicon_and_skips_node_modules() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("node_modules/pkg")).expect("mkdir");
        std::fs::write(root.join("node_modules/pkg/favicon.svg"), "<svg/>").expect("write");
        std::fs::create_dir_all(root.join("crm/public")).expect("mkdir");
        std::fs::write(root.join("crm/public/favicon.svg"), "<svg/>").expect("write");
        std::fs::write(root.join("logo.png"), "png").expect("write");
        let icon = find_icon(root).expect("icon");
        assert!(icon.ends_with("crm/public/favicon.svg"));
    }
}
