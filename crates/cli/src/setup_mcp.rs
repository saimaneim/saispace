//! `sai setup <client>`: registers the saispace MCP server in each client's
//! own config file. Idempotent, backs the file up once (`.bak`) and leaves
//! every other server untouched.

use crate::setup::{read_settings_object, write_json_atomic_with_backup};
use crate::{CliError, CliResult};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const SERVER: &str = "saispace";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    Cursor,
    Windsurf,
    Vscode,
    Codex,
    GeminiCli,
    Antigravity,
}

impl Client {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor",
            Self::Windsurf => "Windsurf",
            Self::Vscode => "VS Code",
            Self::Codex => "Codex",
            Self::GeminiCli => "Gemini CLI",
            Self::Antigravity => "Antigravity",
        }
    }

    /// Name the server reports as author of what it saves.
    fn agent_name(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Windsurf => "windsurf",
            Self::Vscode => "vscode",
            Self::Codex => "codex",
            Self::GeminiCli => "gemini-cli",
            Self::Antigravity => "antigravity",
        }
    }

    /// The client's MCP config file.
    #[must_use]
    pub fn config_path(self, home: &Path) -> PathBuf {
        let codex = std::env::var_os("CODEX_HOME")
            .filter(|v| !v.is_empty())
            .map_or_else(|| home.join(".codex"), PathBuf::from);
        match self {
            Self::Cursor => home.join(".cursor/mcp.json"),
            Self::Windsurf => home.join(".codeium/windsurf/mcp_config.json"),
            Self::Vscode => vscode_user_dir(home).join("mcp.json"),
            Self::Codex => codex.join("config.toml"),
            Self::GeminiCli => home.join(".gemini/settings.json"),
            Self::Antigravity => home.join(".gemini/config/mcp_config.json"),
        }
    }

    /// Key holding the servers map: VS Code uses `servers`, the rest
    /// `mcpServers`.
    fn servers_key(self) -> &'static str {
        if self == Self::Vscode {
            "servers"
        } else {
            "mcpServers"
        }
    }
}

fn vscode_user_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Code/User")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map_or_else(
            || home.join("AppData/Roaming/Code/User"),
            |d| PathBuf::from(d).join("Code/User"),
        )
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map_or_else(|| home.join(".config"), PathBuf::from)
            .join("Code/User")
    }
}

/// `saispace-mcp` next to the running `sai` when present (GUI apps often
/// lack the shell's PATH), else the bare name.
#[must_use]
pub fn server_command() -> String {
    std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name(format!("saispace-mcp{}", std::env::consts::EXE_SUFFIX)))
        .filter(|p| p.is_file())
        .map_or_else(
            || "saispace-mcp".to_string(),
            |p| p.to_string_lossy().into_owned(),
        )
}

fn json_entry(client: Client, command: &str) -> Value {
    let mut entry = json!({
        "command": command,
        "args": [],
        "env": { "SAISPACE_AGENT_NAME": client.agent_name() },
    });
    if client == Client::Vscode {
        entry["type"] = json!("stdio");
    }
    entry
}

fn toml_block(command: &str) -> String {
    let quoted = |s: &str| serde_json::to_string(s).unwrap_or_default();
    format!(
        "[mcp_servers.{SERVER}]\ncommand = {}\nargs = []\nenv = {{ SAISPACE_AGENT_NAME = \"codex\" }}\n",
        quoted(command)
    )
}

/// `text` without the `[mcp_servers.saispace]` table (and its subtables).
fn without_toml_block(text: &str) -> String {
    let header = format!("[mcp_servers.{SERVER}]");
    let sub = format!("[mcp_servers.{SERVER}.");
    let mut out = String::new();
    let mut skipping = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            skipping = trimmed == header || trimmed.starts_with(&sub);
        }
        if !skipping {
            out.push_str(line);
        }
    }
    out
}

/// Adds or refreshes the entry. Returns whether the file changed.
///
/// # Errors
///
/// Returns an error if the file is not valid JSON/an object or cannot be written.
pub fn install(client: Client, path: &Path, command: &str) -> CliResult<bool> {
    if client == Client::Codex {
        let current = fs::read_to_string(path).unwrap_or_default();
        let mut next = without_toml_block(&current).trim_end().to_string();
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(&toml_block(command));
        if next == current {
            return Ok(false);
        }
        backup_once(path)?;
        crate::setup_rules::write_atomic(path, &next)?;
        return Ok(true);
    }
    let mut root = read_settings_object(path)?;
    let entry = json_entry(client, command);
    let obj = root
        .as_object_mut()
        .ok_or_else(|| CliError::Usage(format!("{} no es un objeto JSON", path.display())))?;
    let servers = obj
        .entry(client.servers_key())
        .or_insert_with(|| Value::Object(Map::new()));
    let servers = servers.as_object_mut().ok_or_else(|| {
        CliError::Usage(format!(
            "{}: «{}» no es un objeto",
            path.display(),
            client.servers_key()
        ))
    })?;
    if servers.get(SERVER) == Some(&entry) {
        return Ok(false);
    }
    servers.insert(SERVER.to_string(), entry);
    write_json_atomic_with_backup(path, &root)?;
    Ok(true)
}

/// Removes the entry. Returns whether it was there.
///
/// # Errors
///
/// Returns an error if the file cannot be read or written.
pub fn uninstall(client: Client, path: &Path) -> CliResult<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if client == Client::Codex {
        let current = fs::read_to_string(path)?;
        let next = without_toml_block(&current);
        if next == current {
            return Ok(false);
        }
        backup_once(path)?;
        crate::setup_rules::write_atomic(path, &next)?;
        return Ok(true);
    }
    let mut root = read_settings_object(path)?;
    let removed = root
        .get_mut(client.servers_key())
        .and_then(Value::as_object_mut)
        .and_then(|s| s.remove(SERVER))
        .is_some();
    if removed {
        write_json_atomic_with_backup(path, &root)?;
    }
    Ok(removed)
}

fn backup_once(path: &Path) -> CliResult<()> {
    if path.exists() {
        let backup = PathBuf::from(format!("{}.bak", path.display()));
        if !backup.exists() {
            fs::copy(path, &backup)?;
        }
    }
    Ok(())
}

/// Snippets to paste into any other MCP client.
#[must_use]
pub fn manual(command: &str) -> String {
    let json = serde_json::to_string_pretty(&json!({
        "mcpServers": { SERVER: {
            "command": command,
            "args": [],
            "env": { "SAISPACE_AGENT_NAME": "<tu-cliente>" },
        }}
    }))
    .unwrap_or_default();
    format!(
        "JSON (Cursor, Windsurf, Claude Desktop, Gemini y la mayoría):\n{json}\n\nTOML (Codex):\n{}",
        toml_block(command)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_clients_keep_other_servers_and_are_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp.json");
        fs::write(
            &path,
            r#"{"mcpServers":{"otro":{"command":"x"}},"theme":"dark"}"#,
        )
        .expect("seed");
        assert!(install(Client::Cursor, &path, "saispace-mcp").expect("install"));
        assert!(!install(Client::Cursor, &path, "saispace-mcp").expect("again"));
        let v: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(v["mcpServers"]["otro"]["command"], "x");
        assert_eq!(
            v["mcpServers"]["saispace"]["env"]["SAISPACE_AGENT_NAME"],
            "cursor"
        );
        assert_eq!(v["theme"], "dark");
        assert!(dir.path().join("mcp.json.bak").exists());
        assert!(uninstall(Client::Cursor, &path).expect("uninstall"));
        let v: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert!(v["mcpServers"].get("saispace").is_none());

        let vscode = dir.path().join("vscode.json");
        install(Client::Vscode, &vscode, "saispace-mcp").expect("vscode");
        let v: Value =
            serde_json::from_str(&fs::read_to_string(&vscode).expect("read")).expect("json");
        assert_eq!(v["servers"]["saispace"]["type"], "stdio");
    }

    #[test]
    fn codex_toml_block_replaces_itself_and_keeps_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        let seed = "model = \"o3\"\n\n[mcp_servers.saispace]\ncommand = \"viejo\"\n\n[mcp_servers.saispace.env]\nA = \"1\"\n\n[mcp_servers.otro]\ncommand = \"x\"\n";
        fs::write(&path, seed).expect("seed");
        assert!(install(Client::Codex, &path, "/bin/saispace-mcp").expect("install"));
        assert!(!install(Client::Codex, &path, "/bin/saispace-mcp").expect("again"));
        let text = fs::read_to_string(&path).expect("read");
        assert_eq!(text.matches("[mcp_servers.saispace]").count(), 1, "{text}");
        assert!(!text.contains("viejo") && !text.contains("[mcp_servers.saispace.env]"));
        assert!(text.contains("model = \"o3\"") && text.contains("[mcp_servers.otro]"));
        text.parse::<toml::Table>().expect("valid toml");
        assert!(uninstall(Client::Codex, &path).expect("uninstall"));
        assert!(
            !fs::read_to_string(&path)
                .expect("read")
                .contains("saispace")
        );
    }
}
