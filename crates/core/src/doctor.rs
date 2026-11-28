//! Read-only health report shared by `sai doctor` and the MCP `doctor` tool.
//! It opens the database read-only and only stats or reads config files, so
//! running it never changes anything.

use crate::db::Db;
use crate::error::CoreResult;
use crate::paths::vault_key_path_for;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub version: String,
    pub db_path: String,
    pub db_bytes: u64,
    pub wal_bytes: u64,
    pub project: Option<String>,
    pub checks: Vec<Check>,
}

impl DoctorReport {
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }
}

fn check(name: &str, ok: bool, detail: impl Into<String>) -> Check {
    Check {
        name: name.to_string(),
        ok,
        detail: detail.into(),
    }
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |m| m.len())
}

fn wal_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push("-wal");
    PathBuf::from(name)
}

/// Builds the report. `deep` runs `integrity_check` instead of `quick_check`.
///
/// # Errors
///
/// Returns an error only if the database file cannot be opened at all; every
/// other problem becomes a failed check.
pub async fn run(
    db_path: &Path,
    version: &str,
    cwd: &Path,
    home: Option<&Path>,
    deep: bool,
) -> CoreResult<DoctorReport> {
    let db = Db::open_read_only(db_path).await?;
    let pool = db.pool();
    let mut checks = Vec::new();

    let pragma = if deep {
        "PRAGMA integrity_check"
    } else {
        "PRAGMA quick_check"
    };
    let integrity: Vec<String> = sqlx::query_scalar(pragma)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|err| vec![err.to_string()]);
    let ok = integrity.len() == 1 && integrity[0] == "ok";
    checks.push(check(
        "integridad",
        ok,
        if ok {
            "ok".into()
        } else {
            integrity.join("; ")
        },
    ));

    let applied: HashSet<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1")
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();
    let pending = sqlx::migrate!("./migrations")
        .iter()
        .filter(|m| !applied.contains(&m.version))
        .count();
    checks.push(check(
        "migraciones",
        pending == 0,
        if pending == 0 {
            "al día".to_string()
        } else {
            format!("{pending} pendientes: se aplican al abrir con sai o la app")
        },
    ));

    let counts: Result<(i64, i64), _> = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM memories),
                (SELECT COUNT(*) FROM search_index WHERE kind = 'memory')",
    )
    .fetch_one(pool)
    .await;
    checks.push(match counts {
        Ok((rows, indexed)) => check(
            "índice de búsqueda",
            rows == indexed,
            format!("{indexed} de {rows} memorias indexadas"),
        ),
        Err(err) => check("índice de búsqueda", false, err.to_string()),
    });

    checks.push(vault_key_check(&vault_key_path_for(db_path)));

    let project = db
        .find_project_for_path(cwd)
        .await
        .ok()
        .flatten()
        .map(|p| p.name);

    if let Some(home) = home {
        checks.extend(integration_checks(home));
    }

    Ok(DoctorReport {
        version: version.to_string(),
        db_path: db_path.display().to_string(),
        db_bytes: file_len(db_path),
        wal_bytes: file_len(&wal_path(db_path)),
        project,
        checks,
    })
}

fn vault_key_check(path: &Path) -> Check {
    let Ok(meta) = fs::metadata(path) else {
        return check("clave del vault", true, "sin crear (no hay claves)");
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = meta.permissions().mode() & 0o777;
        // No group or other bits (the low six) set.
        let private = mode.trailing_zeros() >= 6;
        check(
            "clave del vault",
            private,
            if private {
                format!("{mode:o}")
            } else {
                format!("{mode:o}: ejecuta chmod 600 {}", path.display())
            },
        )
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        check("clave del vault", true, "presente")
    }
}

fn contains(path: &Path, needle: &str) -> bool {
    fs::read_to_string(path).is_ok_and(|text| text.contains(needle))
}

/// One check per installed agent: is saispace wired in? Agents whose config
/// dir is missing are skipped.
fn integration_checks(home: &Path) -> Vec<Check> {
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map_or_else(|| home.join(".config"), PathBuf::from);
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map_or_else(|| home.join(".claude"), PathBuf::from);
    let opencode = xdg.join("opencode");
    let mcp = "saispace-mcp";
    let agents: [(&str, PathBuf, bool); 7] = [
        (
            "Claude Code",
            claude.clone(),
            contains(&claude.join("settings.json"), "sai memory context")
                && (contains(&home.join(".claude.json"), mcp)
                    || contains(&claude.join(".claude.json"), mcp)),
        ),
        (
            "OpenCode",
            opencode.clone(),
            opencode.join("plugins/saispace.ts").exists()
                && (contains(&opencode.join("opencode.json"), mcp)
                    || contains(&opencode.join("opencode.jsonc"), mcp)),
        ),
        (
            "Codex",
            home.join(".codex"),
            contains(&home.join(".codex/config.toml"), mcp),
        ),
        (
            "Gemini CLI",
            home.join(".gemini"),
            contains(&home.join(".gemini/settings.json"), mcp),
        ),
        (
            "Antigravity",
            home.join(".gemini/config"),
            contains(&home.join(".gemini/config/mcp_config.json"), mcp),
        ),
        (
            "Windsurf",
            home.join(".codeium/windsurf"),
            contains(&home.join(".codeium/windsurf/mcp_config.json"), mcp),
        ),
        (
            "Cursor",
            home.join(".cursor"),
            contains(&home.join(".cursor/mcp.json"), mcp),
        ),
    ];
    agents
        .into_iter()
        .filter(|(_, dir, _)| dir.is_dir())
        .map(|(name, _, wired)| {
            check(
                name,
                wired,
                if wired {
                    "conectado"
                } else {
                    "sin conectar: ejecuta sai setup <agente>"
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn healthy_database_passes_and_reports_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("saispace.db");
        let db = Db::open(&db_path).await.expect("open");
        db.create_project(crate::models::CreateProjectInput {
            name: "demo".into(),
            path: dir.path().display().to_string(),
        })
        .await
        .expect("project");
        drop(db);

        let report = run(&db_path, "0.1.0", &dir.path().join("src"), None, false)
            .await
            .expect("report");
        assert!(report.healthy(), "{:?}", report.checks);
        assert_eq!(report.project.as_deref(), Some("demo"));
        assert!(report.db_bytes > 0);
    }

    #[cfg(unix)]
    #[test]
    fn world_readable_vault_key_fails() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let key = dir.path().join("vault.key");
        fs::write(&key, "k").expect("key");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).expect("chmod");
        assert!(!vault_key_check(&key).ok);
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).expect("chmod");
        assert!(vault_key_check(&key).ok);
    }
}
