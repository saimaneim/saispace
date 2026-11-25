use crate::error::{CoreError, CoreResult};
use crate::models::DetectedTechnology;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    "vendor",
];
const MAX_DEPTH: usize = 4;
const MAX_MANIFESTS: usize = 64;

/// # Errors
///
/// Returns an error if the path is not a directory or its manifests cannot be read.
pub fn detect_technologies(root: &Path) -> CoreResult<Vec<DetectedTechnology>> {
    if !root.is_dir() {
        return Err(CoreError::Validation(format!(
            "project path is not a directory: {}",
            root.display()
        )));
    }
    let mut manifests = Vec::new();
    collect_manifests(root, 0, &mut manifests);

    let lock_versions = cargo_lock_versions(root);
    let mut found: BTreeMap<(String, String), DetectedTechnology> = BTreeMap::new();

    for manifest in manifests {
        let Some(file_name) = manifest.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let source = manifest
            .strip_prefix(root)
            .unwrap_or(&manifest)
            .to_string_lossy()
            .into_owned();
        let items = match file_name {
            "package.json" => parse_package_json(&manifest, &source),
            "Cargo.toml" => parse_cargo_toml(&manifest, &source, &lock_versions),
            "pyproject.toml" => parse_pyproject(&manifest, &source),
            "requirements.txt" => parse_requirements(&manifest, &source),
            "go.mod" => parse_go_mod(&manifest, &source),
            _ => Vec::new(),
        };
        for item in items {
            found
                .entry((item.ecosystem.clone(), item.name.clone()))
                .or_insert(item);
        }
    }

    Ok(found.into_values().collect())
}

fn collect_manifests(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_MANIFESTS {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !IGNORED_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                subdirs.push(path);
            }
        } else if matches!(
            name.as_str(),
            "package.json" | "Cargo.toml" | "pyproject.toml" | "requirements.txt" | "go.mod"
        ) {
            out.push(path);
        }
    }
    subdirs.sort();
    for sub in subdirs {
        collect_manifests(&sub, depth + 1, out);
    }
}

fn make(name: &str, version: &str, ecosystem: &str, source: &str) -> DetectedTechnology {
    let version = version.trim().to_string();
    let docs_url = match ecosystem {
        "npm" => format!("https://www.npmjs.com/package/{name}/v/{version}"),
        "cargo" => format!("https://docs.rs/{name}/{version}"),
        "pypi" => format!("https://pypi.org/project/{name}/{version}/"),
        "go" => format!("https://pkg.go.dev/{name}@{version}"),
        _ => String::new(),
    };
    DetectedTechnology {
        name: name.to_string(),
        version,
        ecosystem: ecosystem.to_string(),
        source: source.to_string(),
        docs_url,
    }
}

fn clean_range(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(['^', '~', '=', '>', '<', 'v', ' '])
        .to_string()
}

fn parse_package_json(path: &Path, source: &str) -> Vec<DetectedTechnology> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for key in ["dependencies", "devDependencies"] {
        let Some(map) = json.get(key).and_then(|v| v.as_object()) else {
            continue;
        };
        for (name, range) in map {
            let range = range.as_str().unwrap_or("");
            if range.starts_with("workspace:") || range.starts_with("file:") {
                continue;
            }
            let installed = installed_npm_version(dir, name);
            let version = installed.unwrap_or_else(|| clean_range(range));
            if !version.is_empty() {
                out.push(make(name, &version, "npm", source));
            }
        }
    }
    out
}

fn installed_npm_version(dir: &Path, name: &str) -> Option<String> {
    let mut current = Some(dir);
    while let Some(d) = current {
        let candidate = d.join("node_modules").join(name).join("package.json");
        if let Ok(text) = fs::read_to_string(&candidate)
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(v) = json.get("version").and_then(|v| v.as_str())
        {
            return Some(v.to_string());
        }
        current = d.parent();
    }
    None
}

fn cargo_lock_versions(root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(text) = fs::read_to_string(root.join("Cargo.lock")) else {
        return map;
    };
    let Ok(value) = text.parse::<toml::Table>() else {
        return map;
    };
    if let Some(packages) = value.get("package").and_then(|p| p.as_array()) {
        for pkg in packages {
            if let (Some(name), Some(version)) = (
                pkg.get("name").and_then(|v| v.as_str()),
                pkg.get("version").and_then(|v| v.as_str()),
            ) {
                map.entry(name.to_string())
                    .or_insert_with(|| version.to_string());
            }
        }
    }
    map
}

fn parse_cargo_toml(
    path: &Path,
    source: &str,
    lock: &HashMap<String, String>,
) -> Vec<DetectedTechnology> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };

    let mut sections: Vec<&toml::Table> = Vec::new();
    for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = table.get(key).and_then(|v| v.as_table()) {
            sections.push(t);
        }
    }
    if let Some(t) = table
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|v| v.as_table())
    {
        sections.push(t);
    }

    let mut out = Vec::new();
    for section in sections {
        for (name, spec) in section {
            let (declared, is_path) = match spec {
                toml::Value::String(s) => (s.clone(), false),
                toml::Value::Table(t) => (
                    t.get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    t.contains_key("path"),
                ),
                _ => (String::new(), false),
            };
            if is_path {
                continue;
            }
            let version = lock
                .get(name)
                .cloned()
                .unwrap_or_else(|| clean_range(&declared));
            if !version.is_empty() {
                out.push(make(name, &version, "cargo", source));
            }
        }
    }
    out
}

fn split_requirement(line: &str) -> Option<(String, String)> {
    let line = line.split('#').next()?.trim();
    if line.is_empty() || line.starts_with('-') {
        return None;
    }
    let idx = line.find(['=', '>', '<', '~', '!', ' ', '[', ';'])?;
    let name = line[..idx].trim();
    let rest = line[idx..].trim_start_matches(['=', '>', '<', '~', '!', ' ']);
    let version = rest.split([',', ';', ' ']).next().unwrap_or("").to_string();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name.to_string(), version))
}

fn parse_requirements(path: &Path, source: &str) -> Vec<DetectedTechnology> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(split_requirement)
        .map(|(name, version)| make(&name, &version, "pypi", source))
        .collect()
}

fn parse_pyproject(path: &Path, source: &str) -> Vec<DetectedTechnology> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(deps) = table
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep in deps.iter().filter_map(|d| d.as_str()) {
            if let Some((name, version)) = split_requirement(dep) {
                out.push(make(&name, &version, "pypi", source));
            }
        }
    }
    if let Some(deps) = table
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (name, spec) in deps {
            if name == "python" {
                continue;
            }
            let declared = match spec {
                toml::Value::String(s) => s.clone(),
                toml::Value::Table(t) => t
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                _ => String::new(),
            };
            let version = clean_range(&declared);
            if !version.is_empty() {
                out.push(make(name, &version, "pypi", source));
            }
        }
    }
    out
}

fn parse_go_mod(path: &Path, source: &str) -> Vec<DetectedTechnology> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("require (") {
            in_block = true;
            continue;
        }
        if in_block && line == ")" {
            in_block = false;
            continue;
        }
        let spec = if in_block {
            Some(line)
        } else {
            line.strip_prefix("require ")
        };
        if let Some(spec) = spec {
            let spec = spec.split("//").next().unwrap_or("").trim();
            let mut parts = spec.split_whitespace();
            if let (Some(name), Some(version)) = (parts.next(), parts.next()) {
                out.push(make(name, version.trim_start_matches('v'), "go", source));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_npm_and_cargo_with_lock_versions() {
        let dir = std::env::temp_dir().join(format!("saispace-tech-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(dir.join("app")).unwrap();
        fs::write(
            dir.join("app/package.json"),
            r#"{"dependencies":{"react":"^19.0.0"},"devDependencies":{"vite":"~5.1.0"}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[dependencies]\nserde = \"1.0\"\nlocal = { path = \"../x\" }\n",
        )
        .unwrap();
        fs::write(
            dir.join("Cargo.lock"),
            "[[package]]\nname = \"serde\"\nversion = \"1.0.228\"\n",
        )
        .unwrap();

        let found = detect_technologies(&dir).unwrap();
        let get = |n: &str| {
            found
                .iter()
                .find(|t| t.name == n)
                .map(|t| t.version.clone())
        };
        assert_eq!(get("react").as_deref(), Some("19.0.0"));
        assert_eq!(get("vite").as_deref(), Some("5.1.0"));
        assert_eq!(get("serde").as_deref(), Some("1.0.228"));
        assert!(get("local").is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_missing_directory() {
        let missing =
            std::env::temp_dir().join(format!("saispace-missing-{}", uuid::Uuid::now_v7()));
        assert!(matches!(
            detect_technologies(&missing),
            Err(CoreError::Validation(_))
        ));
    }

    #[test]
    fn parses_requirements_lines() {
        assert_eq!(
            split_requirement("requests==2.31.0 # http"),
            Some(("requests".into(), "2.31.0".into()))
        );
        assert_eq!(split_requirement("-r other.txt"), None);
    }
}
