use crate::error::{CommandError, CommandResult};
use base64::Engine as _;
use saispace_core::CoreError;
use saispace_core::inventory::{AiInventory, scan_ai_inventory};
use serde::Deserialize;
use std::path::PathBuf;

const SVGL_API: &str = "https://api.svgl.app";
const MAX_SVG_BYTES: usize = 256 * 1024;
const MISS_TTL: std::time::Duration = std::time::Duration::from_hours(7 * 24);
const SIMPLE_ICONS_CDN: &str = "https://cdn.simpleicons.org";
const NPM_REGISTRY: &str = "https://registry.npmjs.org";
const FAVICON_SERVICE: &str = "https://www.google.com/s2/favicons";

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

#[derive(Deserialize)]
struct NpmPackage {
    homepage: Option<String>,
    repository: Option<NpmRepository>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NpmRepository {
    Url(String),
    Object { url: String },
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

fn svg_data_url(svg: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    format!("data:image/svg+xml;base64,{encoded}")
}

fn png_data_url(bytes: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:image/png;base64,{encoded}")
}

async fn get_bytes(client: &reqwest::Client, url: &str) -> CommandResult<Option<Vec<u8>>> {
    let response = client.get(url).send().await.map_err(|e| http_error(&e))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let bytes = response.bytes().await.map_err(|e| http_error(&e))?;
    Ok((bytes.len() <= MAX_SVG_BYTES).then(|| bytes.to_vec()))
}

async fn from_svgl(client: &reqwest::Client, query: &str) -> CommandResult<Option<String>> {
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
    let Some(bytes) = get_bytes(client, url).await? else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok((text.contains("<svg") && is_compact(&text)).then_some(text))
}

/// Simple Icons slugs differ from package names in a few known ways.
fn simple_icons_slug(query: &str) -> String {
    let slug = normalize(&query.replace('.', "dot").replace('+', "plus"));
    match slug.as_str() {
        "nodejs" | "node" => "nodedotjs".to_string(),
        "nextjs" | "next" => "nextdotjs".to_string(),
        "vue" | "vuejs" => "vuedotjs".to_string(),
        "three" | "threejs" => "threedotjs".to_string(),
        "rollup" => "rollupdotjs".to_string(),
        "chartjs" => "chartdotjs".to_string(),
        "socketio" => "socketdotio".to_string(),
        "shadcn" => "shadcnui".to_string(),
        "sklearn" => "scikitlearn".to_string(),
        "torch" => "pytorch".to_string(),
        _ => slug,
    }
}

/// Whether the SVG is roughly square. Wordmarks (`SQLite` on svgl is 512x228)
/// shrink to an unreadable smudge in a 20px slot, so they are skipped.
fn is_compact(svg: &str) -> bool {
    let attr = |name: &str| -> Option<&str> {
        let start = svg.find(&format!("{name}=\""))? + name.len() + 2;
        svg[start..].split('"').next()
    };
    let size = attr("viewBox")
        .and_then(|vb| {
            let n: Vec<f64> = vb
                .split([' ', ','])
                .filter_map(|v| v.parse().ok())
                .collect();
            (n.len() == 4).then(|| (n[2], n[3]))
        })
        .or_else(|| {
            let num = |v: &str| v.trim_end_matches("px").parse::<f64>().ok();
            Some((num(attr("width")?)?, num(attr("height")?)?))
        });
    size.is_none_or(|(w, h)| w > 0.0 && h > 0.0 && w.max(h) / w.min(h) <= 1.6)
}

/// Simple Icons ships each logo in its brand color; near-black brands would
/// vanish on the dark UI, so those are drawn light instead.
fn readable_on_dark(svg: &str) -> String {
    let Some(start) = svg.find("fill=\"#") else {
        return svg.to_string();
    };
    let hex = &svg[start + 7..];
    let Some(color) = hex.get(..6) else {
        return svg.to_string();
    };
    let channel = |i: usize| u8::from_str_radix(&color[i..i + 2], 16).unwrap_or(255);
    let luminance = 0.2126 * f64::from(channel(0))
        + 0.7152 * f64::from(channel(2))
        + 0.0722 * f64::from(channel(4));
    if luminance < 60.0 {
        svg.replacen(&format!("fill=\"#{color}\""), "fill=\"#E5E5E5\"", 1)
    } else {
        svg.to_string()
    }
}

async fn from_simple_icons(client: &reqwest::Client, query: &str) -> CommandResult<Option<String>> {
    let slug = simple_icons_slug(query);
    if slug.is_empty() {
        return Ok(None);
    }
    let url = format!("{SIMPLE_ICONS_CDN}/{slug}");
    let Some(bytes) = get_bytes(client, &url).await? else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(text.contains("<svg").then(|| readable_on_dark(&text)))
}

/// Last resort for npm packages without a logo in either library: the
/// favicon of the package homepage, or the GitHub owner's avatar.
async fn from_npm_homepage(
    client: &reqwest::Client,
    package: &str,
) -> CommandResult<Option<Vec<u8>>> {
    if package.is_empty() || package.len() > 214 || package.contains(['?', '#', ' ']) {
        return Ok(None);
    }
    let url = format!("{NPM_REGISTRY}/{}", package.replace('/', "%2f"));
    let Ok(response) = client.get(&url).send().await else {
        return Ok(None);
    };
    let Ok(info) = response.json::<NpmPackage>().await else {
        return Ok(None);
    };
    let repo = info.repository.map(|r| match r {
        NpmRepository::Url(url) | NpmRepository::Object { url } => url,
    });
    let homepage = info.homepage.or(repo).unwrap_or_default();
    let Ok(parsed) = reqwest::Url::parse(homepage.trim_start_matches("git+")) else {
        return Ok(None);
    };
    let Some(host) = parsed.host_str() else {
        return Ok(None);
    };
    let icon_url = if host == "github.com" {
        let owner = parsed
            .path_segments()
            .and_then(|mut s| s.next())
            .unwrap_or("");
        if owner.is_empty() {
            return Ok(None);
        }
        format!("https://github.com/{owner}.png?size=64")
    } else {
        format!("{FAVICON_SERVICE}?domain={host}&sz=64")
    };
    get_bytes(client, &icon_url).await
}

enum Icon {
    Svg(String),
    Png(Vec<u8>),
}

async fn fetch_icon(query: &str, package: Option<&str>) -> CommandResult<Option<Icon>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("saispace")
        .build()
        .map_err(|e| http_error(&e))?;
    if let Some(svg) = from_svgl(&client, query).await? {
        return Ok(Some(Icon::Svg(svg)));
    }
    if let Some(svg) = from_simple_icons(&client, query).await? {
        return Ok(Some(Icon::Svg(svg)));
    }
    if let Some(package) = package
        && let Some(png) = from_npm_homepage(&client, package).await?
    {
        return Ok(Some(Icon::Png(png)));
    }
    Ok(None)
}

/// Logo of a technology as a data URL for `<img>` (never inlined, so an SVG
/// cannot run scripts). `package` is the npm name, used only as a fallback.
///
/// # Errors
///
/// Returns an error if the icon cache or the remote lookup fails.
#[tauri::command]
pub async fn tech_icon(name: String, package: Option<String>) -> CommandResult<Option<String>> {
    let key = normalize(&name);
    if key.is_empty() || key.len() > 64 {
        return Ok(None);
    }
    let dir = icon_cache_dir()?;
    let svg = dir.join(format!("{key}.svg"));
    let png = dir.join(format!("{key}.png"));
    // `.miss` replaces the old `.none` marker so earlier misses are retried
    // against the new sources.
    let miss = dir.join(format!("{key}.miss"));
    if let Ok(text) = tokio::fs::read_to_string(&svg).await {
        if is_compact(&text) {
            return Ok(Some(svg_data_url(&text)));
        }
        // Cached before wordmarks were rejected: look it up again.
        tokio::fs::remove_file(&svg).await.ok();
    }
    if let Ok(bytes) = tokio::fs::read(&png).await {
        return Ok(Some(png_data_url(&bytes)));
    }
    // Misses expire so an icon added upstream later is picked up without
    // clearing the cache by hand.
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
    let icon = fetch_icon(&name, package.as_deref()).await?;
    tokio::fs::create_dir_all(&dir).await?;
    Ok(match icon {
        Some(Icon::Svg(text)) => {
            tokio::fs::write(&svg, &text).await?;
            Some(svg_data_url(&text))
        }
        Some(Icon::Png(bytes)) => {
            tokio::fs::write(&png, &bytes).await?;
            Some(png_data_url(&bytes))
        }
        None => {
            tokio::fs::write(&miss, b"").await?;
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{is_compact, readable_on_dark, simple_icons_slug};

    #[test]
    fn rejects_wordmarks() {
        assert!(!is_compact(r#"<svg viewBox="0 0 512 228"></svg>"#));
        assert!(is_compact(r#"<svg viewBox="0 0 256 256"></svg>"#));
        assert!(is_compact(r#"<svg width="24px" height="20px"></svg>"#));
        assert!(!is_compact(r#"<svg width="300" height="100"></svg>"#));
        assert!(is_compact("<svg></svg>"));
    }

    #[test]
    fn slugs_follow_simple_icons_names() {
        assert_eq!(simple_icons_slug("nodejs"), "nodedotjs");
        assert_eq!(simple_icons_slug("better auth"), "betterauth");
        assert_eq!(simple_icons_slug("tailwindcss"), "tailwindcss");
    }

    #[test]
    fn lightens_near_black_logos_only() {
        let black = r##"<svg fill="#000000"><path/></svg>"##;
        assert!(readable_on_dark(black).contains("#E5E5E5"));
        let cyan = r##"<svg fill="#06B6D4"><path/></svg>"##;
        assert_eq!(readable_on_dark(cyan), cyan);
    }
}
