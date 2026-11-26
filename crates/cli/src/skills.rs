//! `sai skills sync`: exposes `.agents/skills` to harnesses that do not read
//! it. Codex, `OpenCode`, Gemini CLI and Antigravity read `.agents/skills`
//! natively; Claude Code only reads `.claude/skills`, so each skill gets a
//! link there. Existing entries are never overwritten.

use crate::{CliError, CliResult};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SOURCE_DIR: &str = ".agents/skills";
const CLAUDE_DIR: &str = ".claude/skills";
/// Marks a directory `sai` copied because links were not available (Windows
/// without developer mode): it is refreshed on the next sync.
const COPY_MARKER: &str = ".sai-copy";

#[derive(Args)]
pub struct SyncArgs {
    #[arg(
        help = "Sincroniza ~/.agents/skills con ~/.claude/skills en vez del proyecto",
        long
    )]
    global: bool,
    #[arg(help = "Solo comprueba: sale con 1 si falta algo por enlazar", long)]
    check: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Linked(String),
    Copied(String),
    UpToDate(String),
    /// An entry with that name exists and is not ours: left alone.
    Kept(String),
    /// A Claude Code plugin already provides a skill with that name; linking
    /// it again would only repeat its description in every session.
    FromPlugin(String),
    RemovedStale(String),
}

fn skill_names(source: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(source) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().join("SKILL.md").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    names
}

fn points_into(link: &Path, source: &Path) -> bool {
    match (fs::canonicalize(link), fs::canonicalize(source)) {
        (Ok(target), Ok(source)) => target.starts_with(source),
        _ => false,
    }
}

#[cfg(unix)]
fn link_dir(relative_target: &Path, at: &Path) -> std::io::Result<bool> {
    std::os::unix::fs::symlink(relative_target, at).map(|()| true)
}

/// On Windows a directory symlink needs developer mode; without it the
/// skill is copied and marked so the next sync refreshes it.
#[cfg(windows)]
fn link_dir(relative_target: &Path, at: &Path) -> std::io::Result<bool> {
    if std::os::windows::fs::symlink_dir(relative_target, at).is_ok() {
        return Ok(true);
    }
    let absolute = at.parent().map_or_else(
        || relative_target.to_path_buf(),
        |p| p.join(relative_target),
    );
    copy_dir(&absolute, at)?;
    fs::write(at.join(COPY_MARKER), "")?;
    Ok(false)
}

#[cfg(windows)]
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Plans and (unless `check`) applies the links from `root/.agents/skills`
/// into `root/.claude/skills`.
///
/// # Errors
///
/// Returns an error if a directory cannot be read or a link cannot be made.
pub fn sync(root: &Path, check: bool, from_plugins: &[String]) -> CliResult<Vec<Outcome>> {
    let source = root.join(SOURCE_DIR);
    let target_dir = root.join(CLAUDE_DIR);
    let names = skill_names(&source);
    let mut outcomes = Vec::new();

    for name in &names {
        let at = target_dir.join(name);
        if from_plugins.contains(name) && fs::symlink_metadata(&at).is_err() {
            outcomes.push(Outcome::FromPlugin(name.clone()));
            continue;
        }
        let ours_copy = at.join(COPY_MARKER).is_file();
        if fs::symlink_metadata(&at).is_ok() && !ours_copy {
            outcomes.push(if points_into(&at, &source) {
                Outcome::UpToDate(name.clone())
            } else {
                Outcome::Kept(name.clone())
            });
            continue;
        }
        if check {
            outcomes.push(Outcome::Linked(name.clone()));
            continue;
        }
        if ours_copy {
            fs::remove_dir_all(&at)?;
        }
        fs::create_dir_all(&target_dir)?;
        // Relative, so the link survives moving or cloning the repo.
        let relative = Path::new("../../").join(SOURCE_DIR).join(name);
        if link_dir(&relative, &at)? {
            outcomes.push(Outcome::Linked(name.clone()));
        } else {
            outcomes.push(Outcome::Copied(name.clone()));
        }
    }

    // Links (or marked copies) for skills that no longer exist.
    if let Ok(entries) = fs::read_dir(&target_dir) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if names.contains(&name) {
                continue;
            }
            let path = entry.path();
            let is_link = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
            let dangling_ours = is_link
                && fs::read_link(&path).is_ok_and(|t| t.to_string_lossy().contains(SOURCE_DIR));
            if dangling_ours || path.join(COPY_MARKER).is_file() {
                if !check {
                    if is_link {
                        fs::remove_file(&path)?;
                    } else {
                        fs::remove_dir_all(&path)?;
                    }
                }
                outcomes.push(Outcome::RemovedStale(name));
            }
        }
    }
    Ok(outcomes)
}

/// Skill names shipped by the installed Claude Code plugins, read from
/// `~/.claude/plugins/installed_plugins.json`. Missing or unreadable files
/// mean no plugins.
fn plugin_skills(home: &Path) -> Vec<String> {
    let index = home.join(".claude/plugins/installed_plugins.json");
    let Ok(text) = fs::read_to_string(index) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut names: Vec<String> = json
        .get("plugins")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|plugins| plugins.values())
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .filter_map(|install| install.get("installPath")?.as_str())
        .flat_map(|path| skill_names(&Path::new(path).join("skills")))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn root_for(global: bool) -> CliResult<PathBuf> {
    if global {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| CliError::Usage("HOME is not set".to_string()));
    }
    Ok(saispace_rules::find_root(&std::env::current_dir()?))
}

pub fn run(args: &SyncArgs) -> CliResult<ExitCode> {
    let root = root_for(args.global)?;
    // Plugins are user-level, so they only shadow the global sync.
    let from_plugins = if args.global {
        plugin_skills(&root)
    } else {
        Vec::new()
    };
    let outcomes = sync(&root, args.check, &from_plugins)?;
    let mut pending = false;
    for outcome in &outcomes {
        match outcome {
            Outcome::Linked(name) if args.check => {
                pending = true;
                crate::out!("falta enlazar   {name}");
            }
            Outcome::Linked(name) => crate::out!("enlazada        {name}"),
            Outcome::Copied(name) => {
                crate::out!("copiada         {name} (sin permiso para enlaces)");
            }
            Outcome::UpToDate(name) => crate::out!("al día          {name}"),
            Outcome::Kept(name) => {
                crate::out!("sin tocar       {name} (ya existe en {CLAUDE_DIR} y no es nuestra)");
            }
            Outcome::FromPlugin(name) => {
                crate::out!("omitida         {name} (ya la trae un plugin de Claude)");
            }
            Outcome::RemovedStale(name) => {
                pending |= args.check;
                crate::out!(
                    "{}{name}",
                    if args.check {
                        "sobra           "
                    } else {
                        "quitada         "
                    }
                );
            }
        }
    }
    if outcomes.is_empty() {
        crate::out!("sin skills en {}", root.join(SOURCE_DIR).display());
    }
    Ok(if pending {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::{Outcome, plugin_skills, sync};
    use std::fs;

    fn skill(root: &std::path::Path, name: &str) {
        let dir = root.join(".agents/skills").join(name);
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("SKILL.md"), "---\nname: x\ndescription: y\n---\n").expect("write");
    }

    #[test]
    fn links_missing_skills_and_never_overwrites() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        skill(root, "alpha");
        skill(root, "beta");
        fs::create_dir_all(root.join(".claude/skills/beta")).expect("own beta");

        assert_eq!(
            sync(root, true, &[]).expect("check"),
            vec![
                Outcome::Linked("alpha".into()),
                Outcome::Kept("beta".into())
            ]
        );
        assert!(
            !root.join(".claude/skills/alpha").exists(),
            "check writes nothing"
        );

        sync(root, false, &[]).expect("sync");
        assert!(root.join(".claude/skills/alpha/SKILL.md").is_file());
        let link = fs::read_link(root.join(".claude/skills/alpha")).expect("link");
        assert!(link.is_relative());
        assert_eq!(
            sync(root, false, &[]).expect("again"),
            vec![
                Outcome::UpToDate("alpha".into()),
                Outcome::Kept("beta".into())
            ]
        );

        fs::remove_dir_all(root.join(".agents/skills/alpha")).expect("rm");
        assert!(
            sync(root, false, &[])
                .expect("prune")
                .contains(&Outcome::RemovedStale("alpha".into()))
        );
        assert!(fs::symlink_metadata(root.join(".claude/skills/alpha")).is_err());
    }

    #[test]
    fn skips_skills_a_plugin_already_provides() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        skill(root, "alpha");
        skill(root, "gamma");
        let plugin = root.join("plugin-install");
        fs::create_dir_all(plugin.join("skills/gamma")).expect("plugin skill");
        fs::write(plugin.join("skills/gamma/SKILL.md"), "x").expect("write");
        fs::create_dir_all(root.join(".claude/plugins")).expect("plugins dir");
        fs::write(
            root.join(".claude/plugins/installed_plugins.json"),
            format!(
                r#"{{"version":2,"plugins":{{"p@m":[{{"installPath":"{}"}}]}}}}"#,
                plugin.display()
            ),
        )
        .expect("index");

        let from_plugins = plugin_skills(root);
        assert_eq!(from_plugins, vec!["gamma".to_string()]);
        assert_eq!(
            sync(root, false, &from_plugins).expect("sync"),
            vec![
                Outcome::Linked("alpha".into()),
                Outcome::FromPlugin("gamma".into())
            ]
        );
        assert!(fs::symlink_metadata(root.join(".claude/skills/gamma")).is_err());
    }
}
