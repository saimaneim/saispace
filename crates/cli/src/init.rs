//! `sai init [name]`: pins the current repo to a project name with a
//! `.saispace.toml` at its root, so clones and renamed folders keep it.

use crate::{CliError, CliResult};
use saispace_core::models::{CreateProjectInput, UpdateProjectInput};
use saispace_core::{Db, PROJECT_MARKER, default_db_path, detect_project_root};

/// # Errors
///
/// Returns an error if the marker cannot be written or the project saved.
pub fn run(name: Option<&str>) -> CliResult<()> {
    let cwd = std::env::current_dir()?;
    let cwd = cwd.canonicalize().unwrap_or(cwd);
    let root = detect_project_root(&cwd);
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(String::from)
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()))
        .ok_or_else(|| CliError::Usage("indica un nombre: sai init <nombre>".to_string()))?;
    if name.contains('"') || name.contains('\n') {
        return Err(CliError::Usage(
            "el nombre no puede llevar comillas ni saltos de línea".to_string(),
        ));
    }
    let marker = root.join(PROJECT_MARKER);
    std::fs::write(&marker, format!("project = \"{name}\"\n"))?;

    let root_str = root.to_string_lossy().to_string();
    let project = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let db = Db::open(&default_db_path()?).await?;
            let project = match db.get_project_by_path(&root_str).await? {
                Some(p) if p.name == name => p,
                Some(p) => {
                    db.update_project(
                        p.id,
                        UpdateProjectInput {
                            name: Some(name.clone()),
                            ..UpdateProjectInput::default()
                        },
                    )
                    .await?
                }
                None => {
                    db.create_project(CreateProjectInput {
                        name: name.clone(),
                        path: root_str.clone(),
                    })
                    .await?
                }
            };
            CliResult::Ok(project)
        })?;
    crate::out!(
        "{} → proyecto «{}». Haz commit de {PROJECT_MARKER} para que cada clon use este nombre.",
        marker.display(),
        project.name
    );
    Ok(())
}
