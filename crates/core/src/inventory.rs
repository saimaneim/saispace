use serde::Serialize;
use serde_json::Value;
use std::path::Path;

const MAX_INSTRUCTION_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Default, Serialize)]
pub struct AiInventory {
    pub instructions: Vec<InstructionFile>,
    pub skills: Vec<SkillEntry>,
    pub mcp_servers: Vec<McpServerEntry>,
    pub plugins: Vec<PluginEntry>,
    pub hooks: Vec<HookEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstructionFile {
    pub tool: String,
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillEntry {
    pub source: String,
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerEntry {
    pub tool: String,
    pub name: String,
    pub target: String,
    pub env_keys: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginEntry {
    pub tool: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HookEntry {
    pub tool: String,
    pub event: String,
    pub command: String,
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

fn read_json(path: &Path, warnings: &mut Vec<String>) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let cleaned = strip_json_comments(&text);
    match serde_json::from_str(&cleaned) {
        Ok(value) => Some(value),
        Err(e) => {
            warnings.push(format!("{}: {e}", display(path)));
            None
        }
    }
}

fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for skipped in chars.by_ref() {
                    if prev == '*' && skipped == '/' {
                        break;
                    }
                    prev = skipped;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn push_instruction(inv: &mut AiInventory, tool: &str, path: &Path) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let truncated = bytes.len() > MAX_INSTRUCTION_BYTES;
    let slice = &bytes[..bytes.len().min(MAX_INSTRUCTION_BYTES)];
    inv.instructions.push(InstructionFile {
        tool: tool.to_string(),
        path: display(path),
        content: String::from_utf8_lossy(slice).into_owned(),
        truncated,
    });
}

fn frontmatter_description(text: &str) -> String {
    let Some(body) = text.strip_prefix("---") else {
        return String::new();
    };
    let header = body.split("\n---").next().unwrap_or_default();
    let mut lines = header.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(value) = line.strip_prefix("description:") else {
            continue;
        };
        let value = value.trim();
        if !value.is_empty() && !matches!(value, ">" | "|" | ">-" | "|-") {
            return value.trim_matches(['"', '\'']).to_string();
        }
        let mut parts = Vec::new();
        while let Some(next) = lines.peek() {
            if !next.starts_with(' ') && !next.starts_with('\t') {
                break;
            }
            parts.push(next.trim().to_string());
            lines.next();
        }
        return parts.join(" ");
    }
    String::new()
}

fn scan_skills(inv: &mut AiInventory, source: &str, dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut found: Vec<SkillEntry> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let skill_file = entry.path().join("SKILL.md");
            let text = std::fs::read_to_string(&skill_file).ok()?;
            Some(SkillEntry {
                source: source.to_string(),
                name: entry.file_name().to_string_lossy().into_owned(),
                description: frontmatter_description(&text),
                path: display(&entry.path()),
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    inv.skills.extend(found);
}

fn server_target(config: &Value) -> String {
    if let Some(url) = config.get("url").and_then(Value::as_str) {
        return match url.split_once('?') {
            Some((base, _)) => format!("{base}?…"),
            None => url.to_string(),
        };
    }
    let mut parts: Vec<String> = Vec::new();
    match config.get("command") {
        Some(Value::String(cmd)) => parts.push(cmd.clone()),
        Some(Value::Array(items)) => {
            parts.extend(items.iter().filter_map(Value::as_str).map(str::to_string));
        }
        _ => {}
    }
    if let Some(Value::Array(args)) = config.get("args") {
        parts.extend(args.iter().filter_map(Value::as_str).map(str::to_string));
    }
    parts.join(" ")
}

fn env_keys(config: &Value) -> Vec<String> {
    ["env", "environment"]
        .iter()
        .filter_map(|key| config.get(*key).and_then(Value::as_object))
        .flat_map(|map| map.keys().cloned())
        .collect()
}

fn push_servers(inv: &mut AiInventory, tool: &str, servers: &serde_json::Map<String, Value>) {
    for (name, config) in servers {
        if name == "servers" {
            if let Some(nested) = config.as_object() {
                push_servers(inv, tool, nested);
            }
            continue;
        }
        if !config.is_object()
            || inv
                .mcp_servers
                .iter()
                .any(|s| s.tool == tool && &s.name == name)
        {
            continue;
        }
        inv.mcp_servers.push(McpServerEntry {
            tool: tool.to_string(),
            name: name.clone(),
            target: server_target(config),
            env_keys: env_keys(config),
            enabled: config
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| {
                    !config
                        .get("disabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                }),
        });
    }
}

/// Antigravity keeps its MCP servers, skills and plugins under
/// `~/.gemini/config` (its rules file is the shared `~/.gemini/GEMINI.md`).
fn scan_antigravity(inv: &mut AiInventory, home: &Path) {
    let dir = home.join(".gemini").join("config");
    scan_skills(inv, "Antigravity", &dir.join("skills"));
    let mut warnings = Vec::new();
    if let Some(Value::Object(servers)) = read_json(&dir.join("mcp_config.json"), &mut warnings)
        .and_then(|v| v.get("mcpServers").cloned())
    {
        push_servers(inv, "Antigravity", &servers);
    }
    if let Ok(entries) = std::fs::read_dir(dir.join("plugins")) {
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            inv.plugins.push(PluginEntry {
                tool: "Antigravity".to_string(),
                name,
                version: String::new(),
                enabled: true,
            });
        }
    }
    inv.warnings.extend(warnings);
}

fn scan_claude(inv: &mut AiInventory, home: &Path) {
    let claude = home.join(".claude");
    push_instruction(inv, "Claude", &claude.join("CLAUDE.md"));
    scan_skills(inv, "Claude", &claude.join("skills"));

    let mut warnings = Vec::new();
    if let Some(Value::Object(servers)) = read_json(&home.join(".claude.json"), &mut warnings)
        .and_then(|v| v.get("mcpServers").cloned())
    {
        push_servers(inv, "Claude", &servers);
    }

    let settings = read_json(&claude.join("settings.json"), &mut warnings);
    let enabled = settings
        .as_ref()
        .and_then(|s| s.get("enabledPlugins"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(Value::Object(plugins)) = read_json(
        &claude.join("plugins").join("installed_plugins.json"),
        &mut warnings,
    )
    .and_then(|v| v.get("plugins").cloned())
    {
        for (name, installs) in plugins {
            let version = installs
                .get(0)
                .and_then(|i| i.get("version"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            inv.plugins.push(PluginEntry {
                tool: "Claude".to_string(),
                enabled: enabled.get(&name).and_then(Value::as_bool).unwrap_or(false),
                name,
                version,
            });
        }
    }

    if let Some(Value::Object(hooks)) = settings.as_ref().and_then(|s| s.get("hooks")).cloned() {
        for (event, groups) in hooks {
            for group in groups.as_array().into_iter().flatten() {
                for hook in group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(command) = hook.get("command").and_then(Value::as_str) {
                        inv.hooks.push(HookEntry {
                            tool: "Claude".to_string(),
                            event: event.clone(),
                            command: command.to_string(),
                        });
                    }
                }
            }
        }
    }
    inv.warnings.extend(warnings);
}

fn scan_opencode(inv: &mut AiInventory, home: &Path) {
    let dir = home.join(".config").join("opencode");
    push_instruction(inv, "OpenCode", &dir.join("AGENTS.md"));
    scan_skills(inv, "OpenCode", &dir.join("skill"));
    scan_skills(inv, "OpenCode", &dir.join("skills"));
    let mut warnings = Vec::new();
    for file in ["opencode.json", "opencode.jsonc"] {
        if let Some(Value::Object(servers)) =
            read_json(&dir.join(file), &mut warnings).and_then(|v| v.get("mcp").cloned())
        {
            push_servers(inv, "OpenCode", &servers);
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir.join("plugins")) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "ts" || e == "js") {
                inv.plugins.push(PluginEntry {
                    tool: "OpenCode".to_string(),
                    name: path
                        .file_stem()
                        .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                    version: String::new(),
                    enabled: true,
                });
            }
        }
    }
    inv.warnings.extend(warnings);
}

fn scan_others(inv: &mut AiInventory, home: &Path) {
    scan_skills(inv, "Compartidas", &home.join(".agents").join("skills"));
    push_instruction(inv, "Gemini", &home.join(".gemini").join("GEMINI.md"));
    push_instruction(inv, "Codex", &home.join(".codex").join("AGENTS.md"));
    let mut warnings = Vec::new();
    if let Some(Value::Object(servers)) =
        read_json(&home.join(".gemini").join("settings.json"), &mut warnings)
            .and_then(|v| v.get("mcpServers").cloned())
    {
        push_servers(inv, "Gemini", &servers);
    }
    inv.warnings.extend(warnings);
}

#[must_use]
pub fn scan_ai_inventory(home: &Path) -> AiInventory {
    let mut inv = AiInventory::default();
    scan_claude(&mut inv, home);
    scan_opencode(&mut inv, home);
    scan_antigravity(&mut inv, home);
    scan_others(&mut inv, home);
    inv
}
