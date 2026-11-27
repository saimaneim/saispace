//! `sai setup rules`: copies one rules file into every agent's own global
//! instructions file, inside a marked block. Each tool reads its native file
//! (no imports or symlinks it may not support) and text outside the block is
//! never touched.

use crate::{CliError, CliResult};
use std::fs;
use std::path::{Path, PathBuf};

const START: &str = "<!-- saispace:rules:start (edita el origen y ejecuta `sai setup rules`) -->";
const END: &str = "<!-- saispace:rules:end -->";

/// An agent and where it keeps its global instructions.
pub struct Target {
    pub name: &'static str,
    /// The tool's config dir: only installed tools (dir exists) are written.
    pub dir: PathBuf,
    /// The instructions file, or `None` when the tool keeps rules in its own
    /// settings UI and they must be pasted by hand.
    pub file: Option<PathBuf>,
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Known agents, resolved against `home` (env overrides as each tool does).
#[must_use]
pub fn targets(home: &Path) -> Vec<Target> {
    let claude = env_dir("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home.join(".claude"));
    let xdg = env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
    let codex = env_dir("CODEX_HOME").unwrap_or_else(|| home.join(".codex"));
    let windsurf = home.join(".codeium/windsurf");
    let kiro = home.join(".kiro");
    vec![
        Target {
            name: "Claude Code",
            file: Some(claude.join("CLAUDE.md")),
            dir: claude,
        },
        Target {
            name: "OpenCode",
            file: Some(xdg.join("opencode/AGENTS.md")),
            dir: xdg.join("opencode"),
        },
        Target {
            name: "Codex",
            file: Some(codex.join("AGENTS.md")),
            dir: codex,
        },
        Target {
            name: "Gemini CLI y Antigravity",
            file: Some(home.join(".gemini/GEMINI.md")),
            dir: home.join(".gemini"),
        },
        Target {
            name: "Windsurf",
            file: Some(windsurf.join("memories/global_rules.md")),
            dir: windsurf,
        },
        Target {
            name: "Kiro",
            file: Some(kiro.join("steering/saispace-rules.md")),
            dir: kiro,
        },
        Target {
            name: "Cursor",
            file: None,
            dir: home.join(".cursor"),
        },
    ]
}

/// Default rules source: `$XDG_CONFIG_HOME/saispace/rules.md`.
#[must_use]
pub fn default_source(home: &Path) -> PathBuf {
    env_dir("XDG_CONFIG_HOME")
        .unwrap_or_else(|| home.join(".config"))
        .join("saispace/rules.md")
}

/// `text` with the managed block set to `rules` (or removed when `None`).
/// Text outside the block is kept byte for byte; only the blank lines that
/// separate it from the block are normalized.
#[must_use]
pub fn with_block(text: &str, rules: Option<&str>) -> String {
    let outside = match (text.find(START), text.find(END)) {
        (Some(a), Some(b)) if b > a => {
            let before = text[..a].trim_end_matches('\n');
            let after = text[b + END.len()..].trim_start_matches('\n');
            match (before.is_empty(), after.is_empty()) {
                (true, _) => after.to_string(),
                (false, true) => format!("{before}\n"),
                (false, false) => format!("{before}\n\n{after}"),
            }
        }
        _ => text.to_string(),
    };
    let outside = if outside.is_empty() || outside.ends_with('\n') {
        outside
    } else {
        format!("{outside}\n")
    };
    let Some(rules) = rules else {
        return outside;
    };
    let block = format!("{START}\n{}\n{END}\n", rules.trim());
    if outside.trim().is_empty() {
        block
    } else {
        format!("{block}\n{outside}")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Written,
    Unchanged,
    Removed,
    Manual,
    NotInstalled,
    /// The target is the rules source itself: left untouched.
    IsSource,
}

fn not_found_ok<T: Default>(res: std::io::Result<T>) -> std::io::Result<T> {
    match res {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        other => other,
    }
}

/// Replaces `path` through a temporary sibling so a failed write never
/// leaves it truncated. A symlink is written through to its target.
fn write_atomic(path: &Path, content: &str) -> CliResult<()> {
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let parent = real
        .parent()
        .ok_or_else(|| CliError::Usage(format!("ruta sin directorio: {}", real.display())))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".saispace-rules.{}.tmp", std::process::id()));
    let result = fs::write(&tmp, content)
        .and_then(|()| match fs::metadata(&real) {
            Ok(meta) => fs::set_permissions(&tmp, meta.permissions()),
            Err(_) => Ok(()),
        })
        .and_then(|()| fs::rename(&tmp, &real));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

/// Writes (or with `rules = None` removes) the block in one target's file.
///
/// A symlink pointing at the rules source itself is replaced by a regular
/// file, so the tool never reads a file that contains its own block twice.
/// A target that is the source as a regular file is skipped.
///
/// # Errors
///
/// Returns an error if the file cannot be read or written.
pub fn apply(target: &Target, source: &Path, rules: Option<&str>) -> CliResult<Outcome> {
    if !target.dir.is_dir() {
        return Ok(Outcome::NotInstalled);
    }
    let Some(file) = &target.file else {
        return Ok(Outcome::Manual);
    };
    let same_file = matches!(
        (fs::canonicalize(file), fs::canonicalize(source)),
        (Ok(a), Ok(b)) if a == b
    );
    let is_link = fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink());
    let points_at_source = same_file && is_link;
    if same_file && !is_link {
        return Ok(Outcome::IsSource);
    }
    let current = if points_at_source {
        String::new()
    } else {
        not_found_ok(fs::read_to_string(file))?
    };
    let next = with_block(&current, rules);
    if next == current && !points_at_source {
        return Ok(if rules.is_some() {
            Outcome::Unchanged
        } else {
            Outcome::NotInstalled
        });
    }
    if points_at_source {
        fs::remove_file(file)?;
    } else if file.exists() {
        let backup = PathBuf::from(format!("{}.bak", file.display()));
        if !backup.exists() {
            fs::copy(file, &backup)?;
        }
    }
    if next.is_empty() && rules.is_none() {
        not_found_ok(fs::remove_file(file))?;
    } else {
        write_atomic(file, &next)?;
    }
    Ok(if rules.is_some() {
        Outcome::Written
    } else {
        Outcome::Removed
    })
}

/// # Errors
///
/// Returns an error if `HOME` is unset, the source is missing, or a write fails.
pub fn run(from: Option<&Path>, uninstall: bool) -> CliResult<()> {
    let home = env_dir("HOME").ok_or_else(|| CliError::Usage("HOME is not set".to_string()))?;
    let source = from.map_or_else(|| default_source(&home), Path::to_path_buf);
    let rules = if uninstall {
        None
    } else {
        let text = fs::read_to_string(&source).map_err(|_| {
            CliError::Usage(format!(
                "no existe {}: escribe ahí tus reglas o pasa --from <fichero>",
                source.display()
            ))
        })?;
        if text.contains(START) || text.contains(END) {
            return Err(CliError::Usage(
                "el origen contiene un bloque de saispace: usa un fichero propio".to_string(),
            ));
        }
        Some(text)
    };
    for target in targets(&home) {
        let label = target
            .file
            .as_ref()
            .map_or_else(String::new, |f| f.display().to_string());
        match apply(&target, &source, rules.as_deref())? {
            Outcome::Written => crate::out!("{}: actualizado {label}", target.name),
            Outcome::Unchanged => crate::out!("{}: al día", target.name),
            Outcome::Removed => crate::out!("{}: bloque quitado de {label}", target.name),
            Outcome::Manual if rules.is_some() => crate::out!(
                "{}: sin fichero global; pega {} en Settings > Rules",
                target.name,
                source.display()
            ),
            Outcome::IsSource => crate::out!("{}: es el origen, se omite", target.name),
            Outcome::Manual | Outcome::NotInstalled => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_replaces_itself_and_keeps_user_text() {
        let first = with_block("Mis reglas propias\n", Some("- regla A"));
        assert!(first.starts_with(START), "{first}");
        assert!(first.ends_with("Mis reglas propias\n"), "{first}");
        let second = with_block(&first, Some("- regla B"));
        assert!(second.contains("- regla B") && !second.contains("- regla A"));
        assert_eq!(second.matches(START).count(), 1);
        assert_eq!(with_block(&second, Some("- regla B")), second, "idempotent");
        assert_eq!(with_block(&second, None), "Mis reglas propias\n");

        let indented = "    código sangrado\n";
        let synced = with_block(&with_block(indented, Some("- r")), Some("- r"));
        assert_eq!(
            with_block(&synced, None),
            indented,
            "outer text kept byte for byte"
        );
    }

    #[test]
    fn the_source_itself_is_never_rewritten() {
        let home = tempfile::tempdir().expect("tempdir");
        let source = home.path().join(".a/AGENTS.md");
        fs::create_dir_all(home.path().join(".a")).expect("dir");
        fs::write(&source, "- mis reglas\n").expect("source");
        let target = Target {
            name: "a",
            dir: home.path().join(".a"),
            file: Some(source.clone()),
        };
        assert_eq!(
            apply(&target, &source, Some("- mis reglas")).expect("apply"),
            Outcome::IsSource
        );
        assert_eq!(fs::read_to_string(&source).expect("read"), "- mis reglas\n");
    }

    #[test]
    fn apply_writes_installed_tools_only_and_unlinks_source_symlinks() {
        let home = tempfile::tempdir().expect("tempdir");
        let source = home.path().join("rules.md");
        fs::write(&source, "- corto").expect("source");
        let installed = Target {
            name: "a",
            dir: home.path().join(".a"),
            file: Some(home.path().join(".a/AGENTS.md")),
        };
        let absent = Target {
            name: "b",
            dir: home.path().join(".b"),
            file: Some(home.path().join(".b/AGENTS.md")),
        };
        fs::create_dir_all(&installed.dir).expect("dir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, installed.file.as_ref().expect("file"))
            .expect("symlink");

        assert_eq!(
            apply(&installed, &source, Some("- corto")).expect("apply"),
            Outcome::Written
        );
        assert_eq!(
            apply(&absent, &source, Some("- corto")).expect("apply"),
            Outcome::NotInstalled
        );
        let file = installed.file.as_ref().expect("file");
        assert!(
            !fs::symlink_metadata(file)
                .expect("meta")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(&source).expect("source"),
            "- corto",
            "source intact"
        );
        assert_eq!(
            apply(&installed, &source, Some("- corto")).expect("again"),
            Outcome::Unchanged
        );
        assert_eq!(
            apply(&installed, &source, None).expect("remove"),
            Outcome::Removed
        );
        assert!(!file.exists(), "a file that only held the block goes away");
    }
}
