use crate::error::{CommandError, CommandResult};
use crate::error_log::record_activity_or_log;
use chrono::{DateTime, Utc};
use saispace_core::models::{ActionKind, Actor, EntityKind, Project, RecordActivityInput};
use saispace_core::{CoreError, Db};
use serde::{Deserialize, Serialize};
use std::fs::{self, Metadata};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use tauri::State;
use uuid::Uuid;

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_LISTED_FILES: usize = 300;
const MAX_DEPTH: usize = 4;
const IGNORED_DIRS: [&str; 4] = ["node_modules", "target", ".git", "dist"];
const TEXT_EXTENSIONS: [&str; 7] = ["md", "mdc", "txt", "json", "toml", "yml", "yaml"];

const ROOT_FILES: [(&str, ContextFileKind); 9] = [
    ("AGENTS.md", ContextFileKind::Agents),
    ("CLAUDE.md", ContextFileKind::Claude),
    ("GEMINI.md", ContextFileKind::Gemini),
    ("DESIGN.md", ContextFileKind::Design),
    ("CONVENTIONS.md", ContextFileKind::Rules),
    (".cursorrules", ContextFileKind::Rules),
    (".windsurfrules", ContextFileKind::Rules),
    (".clinerules", ContextFileKind::Rules),
    (".github/copilot-instructions.md", ContextFileKind::Rules),
];

const WRITABLE_ROOT_FILES: [&str; 4] = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "DESIGN.md"];

const SCAN_DIRS: [(&str, ContextFileKind); 6] = [
    (".agents", ContextFileKind::AgentsDir),
    (".claude", ContextFileKind::Claude),
    (".gemini", ContextFileKind::Gemini),
    (".cursor/rules", ContextFileKind::Rules),
    (".codex", ContextFileKind::Other),
    (".opencode", ContextFileKind::Other),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextFileKind {
    Agents,
    Claude,
    Gemini,
    Design,
    Rules,
    AgentsDir,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFile {
    pub path: String,
    pub name: String,
    pub kind: ContextFileKind,
    pub size: u64,
    pub modified_at: Option<String>,
    /// Set when the file only points at another one (a symlink, or a stub
    /// like `CLAUDE.md` containing just `@AGENTS.md`), so the UI can tell
    /// copies of the same content apart from real files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ContextLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextLinkVia {
    Symlink,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextLink {
    /// Target relative to the project root (or absolute when outside it).
    pub target: String,
    pub via: ContextLinkVia,
}

/// Stubs larger than this are real documents, not pointers.
const MAX_IMPORT_STUB_BYTES: u64 = 512;

fn link_of(root: &Path, relative: &str, metadata: &Metadata) -> Option<ContextLink> {
    let path = root.join(relative);
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        let target = fs::canonicalize(&path).ok()?;
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let shown = target.strip_prefix(&root).map_or_else(
            |_| target.display().to_string(),
            |p| p.display().to_string(),
        );
        return Some(ContextLink {
            target: shown,
            via: ContextLinkVia::Symlink,
        });
    }
    if metadata.len() > MAX_IMPORT_STUB_BYTES {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    let imports: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if imports.is_empty()
        || !imports
            .iter()
            .all(|line| line.starts_with('@') && line.len() > 1)
    {
        return None;
    }
    Some(ContextLink {
        target: imports
            .iter()
            .map(|line| line.trim_start_matches('@'))
            .collect::<Vec<_>>()
            .join(", "),
        via: ContextLinkVia::Import,
    })
}

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::Core(CoreError::Validation(message.into()))
}

fn outside_root() -> CommandError {
    invalid("Path resolves outside the project root")
}

fn classify_relative(relative: &str) -> Option<ContextFileKind> {
    if let Some((_, kind)) = ROOT_FILES.iter().find(|(path, _)| *path == relative) {
        return Some(*kind);
    }

    let parts: Vec<&str> = relative.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == ".." || IGNORED_DIRS.contains(part))
    {
        return None;
    }

    let (dir, kind) = SCAN_DIRS.iter().find(|(dir, _)| {
        let dir_parts: Vec<&str> = dir.split('/').collect();
        parts.len() > dir_parts.len() && parts[..dir_parts.len()] == dir_parts[..]
    })?;
    let dir_len = dir.split('/').count();

    let file_name = parts[parts.len() - 1];
    let subdirs = parts.len() - dir_len - 1;
    if subdirs > MAX_DEPTH {
        return None;
    }
    let extension = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())?;
    if !TEXT_EXTENSIONS.contains(&extension) {
        return None;
    }
    if *dir == ".claude" && file_name == "settings.local.json" {
        return None;
    }
    Some(*kind)
}

fn validated_kind(relative: &str) -> CommandResult<ContextFileKind> {
    if relative.trim().is_empty() {
        return Err(invalid("Context file path must not be empty"));
    }
    if relative.contains(['\\', '\0']) || Path::new(relative).is_absolute() {
        return Err(invalid("Context file path must be relative and use '/'"));
    }
    if Path::new(relative)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid(
            "Context file path must not contain '..' or '.' segments",
        ));
    }
    classify_relative(relative)
        .ok_or_else(|| invalid(format!("Not a recognized context file: {relative}")))
}

fn canonical_inside(root: &Path, path: &Path) -> CommandResult<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|_| outside_root())?;
    if canonical.starts_with(root) {
        Ok(canonical)
    } else {
        Err(outside_root())
    }
}

fn resolve_within(root: &Path, relative: &str) -> CommandResult<PathBuf> {
    let mut prefix = root.to_path_buf();
    let mut resolved = root.to_path_buf();
    for component in Path::new(relative).components() {
        prefix.push(component);
        if fs::symlink_metadata(&prefix).is_err() {
            return Ok(root.join(relative));
        }
        resolved = canonical_inside(root, &prefix)?;
    }
    Ok(resolved)
}

fn describe(
    root: &Path,
    relative: &str,
    kind: ContextFileKind,
    metadata: &Metadata,
) -> ContextFile {
    ContextFile {
        link: link_of(root, relative, metadata),
        path: relative.to_string(),
        name: relative.rsplit('/').next().unwrap_or(relative).to_string(),
        kind,
        size: metadata.len(),
        modified_at: metadata
            .modified()
            .ok()
            .map(|time| DateTime::<Utc>::from(time).to_rfc3339()),
    }
}

fn root_entry(root: &Path, relative: &str, kind: ContextFileKind) -> Option<ContextFile> {
    let path = canonical_inside(root, &root.join(relative)).ok()?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    Some(describe(root, relative, kind, &metadata))
}

fn collect_dir(
    root: &Path,
    relative_dir: &str,
    depth: usize,
    out: &mut Vec<ContextFile>,
    limit: usize,
) {
    let Ok(read_dir) = fs::read_dir(root.join(relative_dir)) else {
        return;
    };
    let mut entries: Vec<fs::DirEntry> = read_dir.flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        if out.len() >= limit {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        let relative = format!("{relative_dir}/{name}");
        let path = root.join(&relative);
        let is_symlink = entry
            .file_type()
            .is_ok_and(|file_type| file_type.is_symlink());
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            if !is_symlink && depth < MAX_DEPTH {
                collect_dir(root, &relative, depth + 1, out, limit);
            }
            continue;
        }
        let Some(kind) = classify_relative(&relative) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        if canonical_inside(root, &path).is_err() {
            continue;
        }
        out.push(describe(root, &relative, kind, &metadata));
    }
}

const fn root_rank(kind: ContextFileKind) -> u8 {
    match kind {
        ContextFileKind::Agents => 0,
        ContextFileKind::Claude => 1,
        ContextFileKind::Gemini => 2,
        ContextFileKind::Design => 3,
        ContextFileKind::Rules => 4,
        ContextFileKind::AgentsDir | ContextFileKind::Other => 5,
    }
}

fn scan_project(root: &Path) -> Vec<ContextFile> {
    if !root.is_dir() {
        return Vec::new();
    }

    let mut primary: Vec<ContextFile> = ROOT_FILES
        .iter()
        .filter_map(|(relative, kind)| root_entry(root, relative, *kind))
        .collect();
    primary.sort_by(|a, b| {
        root_rank(a.kind)
            .cmp(&root_rank(b.kind))
            .then_with(|| a.path.cmp(&b.path))
    });

    let mut rest = Vec::new();
    let limit = MAX_LISTED_FILES.saturating_sub(primary.len());
    for (dir, _) in SCAN_DIRS {
        collect_dir(root, dir, 0, &mut rest, limit);
    }
    rest.sort_by(|a, b| a.path.cmp(&b.path));

    primary.extend(rest);
    primary
}

fn read_context(root: &Path, relative: &str) -> CommandResult<String> {
    validated_kind(relative)?;
    let target = resolve_within(root, relative)?;
    let metadata = fs::metadata(&target)
        .map_err(|_| invalid(format!("Context file not found: {relative}")))?;
    if !metadata.is_file() {
        return Err(invalid(format!("Not a regular file: {relative}")));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(invalid(format!("Context file exceeds 512 KiB: {relative}")));
    }
    Ok(fs::read_to_string(&target)?)
}

fn write_new_file(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()
}

fn write_context(root: &Path, relative: &str, content: &str) -> CommandResult<ContextFile> {
    let kind = validated_kind(relative)?;
    let is_fixed = ROOT_FILES.iter().any(|(path, _)| *path == relative);
    let exists = fs::metadata(root.join(relative)).is_ok();
    if is_fixed && !exists && !WRITABLE_ROOT_FILES.contains(&relative) {
        return Err(invalid(format!(
            "{relative} does not exist and cannot be created"
        )));
    }
    if u64::try_from(content.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err(invalid("Context file content exceeds 512 KiB"));
    }

    let target = resolve_within(root, relative)?;
    if fs::metadata(&target).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(invalid(format!("Not a regular file: {relative}")));
    }

    let parent = target
        .parent()
        .ok_or_else(|| invalid(format!("Invalid context file path: {relative}")))?;
    fs::create_dir_all(parent)?;
    canonical_inside(root, parent)?;
    let file_name = target
        .file_name()
        .ok_or_else(|| invalid(format!("Invalid context file path: {relative}")))?
        .to_string_lossy()
        .into_owned();

    let temp = parent.join(format!(".{file_name}.{}.tmp", Uuid::now_v7()));
    if let Err(err) = write_new_file(&temp, content).and_then(|()| fs::rename(&temp, &target)) {
        let _ = fs::remove_file(&temp);
        return Err(err.into());
    }

    let metadata = fs::metadata(&target)?;
    Ok(describe(root, relative, kind, &metadata))
}

async fn load_project(db: &Db, project_id: &str) -> CommandResult<Project> {
    let id = Uuid::parse_str(project_id)
        .map_err(|err| invalid(format!("Invalid UUID '{project_id}': {err}")))?;
    db.get_project(id).await?.ok_or_else(|| {
        CommandError::from(CoreError::NotFound {
            entity: "Project",
            id,
        })
    })
}

fn canonical_root(project: &Project) -> CommandResult<PathBuf> {
    fs::canonicalize(&project.path)
        .ok()
        .filter(|root| root.is_dir())
        .ok_or_else(|| invalid(format!("Project path is not a directory: {}", project.path)))
}

/// # Errors
///
/// Returns an error if the project does not exist or its context files cannot be listed.
#[tauri::command]
pub async fn list_context_files(
    project_id: String,
    db: State<'_, Db>,
) -> CommandResult<Vec<ContextFile>> {
    let project = load_project(&db, &project_id).await?;
    let Ok(root) = fs::canonicalize(&project.path) else {
        return Ok(Vec::new());
    };
    Ok(scan_project(&root))
}

/// # Errors
///
/// Returns an error if the file cannot be read.
#[tauri::command]
pub async fn read_context_file(
    project_id: String,
    relative_path: String,
    db: State<'_, Db>,
) -> CommandResult<String> {
    let project = load_project(&db, &project_id).await?;
    let root = canonical_root(&project)?;
    read_context(&root, &relative_path)
}

/// # Errors
///
/// Returns an error if the file cannot be written.
#[tauri::command]
pub async fn write_context_file(
    project_id: String,
    relative_path: String,
    content: String,
    db: State<'_, Db>,
) -> CommandResult<ContextFile> {
    let project = load_project(&db, &project_id).await?;
    let root = canonical_root(&project)?;
    let entry = write_context(&root, &relative_path, &content)?;
    record_activity_or_log(
        &db,
        "write_context_file activity",
        RecordActivityInput {
            project_id: Some(project.id),
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::ContextNotes,
            entity_id: Some(project.id),
            details: Some(serde_json::json!({ "path": relative_path })),
        },
    )
    .await;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("saispace-context-{}", Uuid::now_v7()));
            fs::create_dir_all(&dir).expect("create temp project");
            Self(fs::canonicalize(&dir).expect("canonicalize temp project"))
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, relative: &str, content: &str) {
            let target = self.0.join(relative);
            fs::create_dir_all(target.parent().expect("parent")).expect("create dirs");
            fs::write(target, content).expect("write fixture");
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn classifies_recognized_paths() {
        assert_eq!(
            classify_relative("AGENTS.md"),
            Some(ContextFileKind::Agents)
        );
        assert_eq!(
            classify_relative("CLAUDE.md"),
            Some(ContextFileKind::Claude)
        );
        assert_eq!(
            classify_relative(".cursorrules"),
            Some(ContextFileKind::Rules)
        );
        assert_eq!(
            classify_relative(".github/copilot-instructions.md"),
            Some(ContextFileKind::Rules)
        );
        assert_eq!(
            classify_relative(".agents/skills/a.md"),
            Some(ContextFileKind::AgentsDir)
        );
        assert_eq!(
            classify_relative(".claude/commands/c.md"),
            Some(ContextFileKind::Claude)
        );
        assert_eq!(
            classify_relative(".cursor/rules/r.mdc"),
            Some(ContextFileKind::Rules)
        );
        assert_eq!(
            classify_relative(".codex/config.toml"),
            Some(ContextFileKind::Other)
        );
        assert_eq!(
            classify_relative(".agents/a/b/c/d/x.md"),
            Some(ContextFileKind::AgentsDir)
        );
    }

    #[test]
    fn rejects_unrecognized_paths() {
        assert_eq!(classify_relative("src/main.rs"), None);
        assert_eq!(classify_relative("README.md"), None);
        assert_eq!(classify_relative(".claude/settings.local.json"), None);
        assert_eq!(classify_relative(".agents/a/b/c/d/e/x.md"), None);
        assert_eq!(classify_relative(".agents/node_modules/x.md"), None);
        assert_eq!(classify_relative(".agents/bin.exe"), None);
        assert_eq!(classify_relative(".cursor/other.md"), None);
        assert_eq!(classify_relative("sub/AGENTS.md"), None);
    }

    #[test]
    fn rejects_traversal_absolute_and_empty_paths() {
        assert!(validated_kind("").is_err());
        assert!(validated_kind("   ").is_err());
        assert!(validated_kind("/etc/passwd").is_err());
        assert!(validated_kind("../AGENTS.md").is_err());
        assert!(validated_kind(".agents/../../secret.md").is_err());
        assert!(validated_kind("./AGENTS.md").is_err());
        assert!(validated_kind(".agents\\x.md").is_err());
        assert!(validated_kind("AGENTS.md/").is_err());
        assert!(validated_kind("src/main.rs").is_err());
        assert_eq!(
            validated_kind("AGENTS.md").ok(),
            Some(ContextFileKind::Agents)
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_and_write_refuse_symlinks_leaving_the_root() {
        let project = TempProject::new();
        let outside = TempProject::new();
        outside.write("secret.md", "top secret");
        fs::write(outside.path().join("notes.md"), "outside").expect("fixture");
        std::os::unix::fs::symlink(
            outside.path().join("secret.md"),
            project.path().join("AGENTS.md"),
        )
        .expect("symlink file");
        std::os::unix::fs::symlink(outside.path(), project.path().join(".agents"))
            .expect("symlink dir");

        assert!(read_context(project.path(), "AGENTS.md").is_err());
        assert!(write_context(project.path(), "AGENTS.md", "pwned").is_err());
        assert!(read_context(project.path(), ".agents/notes.md").is_err());
        assert!(write_context(project.path(), ".agents/new.md", "pwned").is_err());
        assert_eq!(
            fs::read_to_string(outside.path().join("secret.md")).expect("secret intact"),
            "top secret"
        );
        assert!(
            scan_project(project.path())
                .iter()
                .all(|f| f.path != "AGENTS.md")
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_through_symlink_inside_root_updates_target() {
        let project = TempProject::new();
        project.write("AGENTS.md", "old");
        std::os::unix::fs::symlink(
            project.path().join("AGENTS.md"),
            project.path().join("CLAUDE.md"),
        )
        .expect("symlink inside root");

        write_context(project.path(), "CLAUDE.md", "new").expect("write through link");
        assert_eq!(
            fs::read_to_string(project.path().join("AGENTS.md")).expect("read target"),
            "new"
        );
        assert!(
            fs::symlink_metadata(project.path().join("CLAUDE.md"))
                .expect("link metadata")
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn write_creates_files_and_nested_dirs_inside_root() {
        let project = TempProject::new();

        let root_entry =
            write_context(project.path(), "AGENTS.md", "# Agents").expect("write root");
        assert_eq!(root_entry.kind, ContextFileKind::Agents);
        assert_eq!(root_entry.size, 8);
        assert_eq!(root_entry.name, "AGENTS.md");

        let nested = write_context(project.path(), ".agents/skills/deep/x.md", "skill")
            .expect("write nested");
        assert_eq!(nested.path, ".agents/skills/deep/x.md");
        assert_eq!(nested.kind, ContextFileKind::AgentsDir);
        assert_eq!(
            read_context(project.path(), ".agents/skills/deep/x.md").expect("read nested"),
            "skill"
        );

        let leftovers: Vec<_> = fs::read_dir(project.path().join(".agents/skills/deep"))
            .expect("read dir")
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn write_only_creates_writable_root_files() {
        let project = TempProject::new();
        assert!(write_context(project.path(), "CONVENTIONS.md", "x").is_err());
        assert!(write_context(project.path(), ".cursorrules", "x").is_err());
        assert!(write_context(project.path(), ".github/copilot-instructions.md", "x").is_err());
        assert!(write_context(project.path(), "CLAUDE.md", "x").is_ok());

        project.write(".cursorrules", "old");
        assert!(write_context(project.path(), ".cursorrules", "new").is_ok());
        assert_eq!(
            read_context(project.path(), ".cursorrules").expect("read"),
            "new"
        );
    }

    #[test]
    fn write_rejects_oversized_content() {
        let project = TempProject::new();
        let big = "a".repeat(usize::try_from(MAX_FILE_BYTES).expect("fits") + 1);
        assert!(write_context(project.path(), "AGENTS.md", &big).is_err());
    }

    #[test]
    fn scan_lists_root_files_first_and_skips_ignored_and_oversized() {
        let project = TempProject::new();
        project.write(".agents/zeta.md", "z");
        project.write(".agents/node_modules/pkg.md", "x");
        project.write(".claude/settings.local.json", "{}");
        project.write(".claude/settings.json", "{}");
        project.write(".cursor/rules/rule.mdc", "r");
        project.write("CLAUDE.md", "c");
        project.write("AGENTS.md", "a");
        project.write(
            ".agents/huge.md",
            &"h".repeat(usize::try_from(MAX_FILE_BYTES).expect("fits") + 1),
        );

        let listed = scan_project(project.path());
        let paths: Vec<&str> = listed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "AGENTS.md",
                "CLAUDE.md",
                ".agents/zeta.md",
                ".claude/settings.json",
                ".cursor/rules/rule.mdc",
            ]
        );
        assert_eq!(listed[0].kind, ContextFileKind::Agents);
        assert_eq!(listed[0].size, 1);
        assert!(listed[0].modified_at.is_some());
    }

    #[test]
    fn scan_of_missing_root_is_empty() {
        let missing = std::env::temp_dir().join(format!("saispace-missing-{}", Uuid::now_v7()));
        assert_eq!(scan_project(&missing), Vec::<ContextFile>::new());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_import_stubs_are_marked_as_links() {
        let project = tempfile::tempdir().expect("tempdir");
        let root = project.path();
        fs::write(root.join("AGENTS.md"), "# Reglas\nUsa bun.\n").expect("agents");
        fs::write(root.join("CLAUDE.md"), "@AGENTS.md\n").expect("claude");
        std::os::unix::fs::symlink("AGENTS.md", root.join("GEMINI.md")).expect("symlink");

        let files = scan_project(root);
        let link = |name: &str| {
            files
                .iter()
                .find(|f| f.path == name)
                .and_then(|f| f.link.clone())
        };
        assert_eq!(link("AGENTS.md"), None);
        assert_eq!(
            link("CLAUDE.md"),
            Some(ContextLink {
                target: "AGENTS.md".into(),
                via: ContextLinkVia::Import
            })
        );
        assert_eq!(
            link("GEMINI.md"),
            Some(ContextLink {
                target: "AGENTS.md".into(),
                via: ContextLinkVia::Symlink
            })
        );
    }
}
