use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::models::{
    ActionKind, Actor, CreateProjectInput, EntityKind, Project, RecordActivityInput,
    UpdateProjectInput,
};
use saispace_core::{CoreError, Db};
use tauri::State;
use uuid::Uuid;

#[must_use]
pub fn canonicalize_path(path: &str) -> String {
    let trimmed = path.trim();
    match std::fs::canonicalize(trimmed) {
        Ok(canonical) => canonical.to_string_lossy().into_owned(),
        Err(_) => trimmed.to_string(),
    }
}

/// # Errors
///
/// Returns an error if the path does not exist or is not a directory.
pub fn validate_project_path(path: &str) -> Result<String, CoreError> {
    let canonical = canonicalize_path(path);
    if !std::path::Path::new(&canonical).is_dir() {
        return Err(CoreError::Validation(
            "La ruta no existe o no es un directorio".to_string(),
        ));
    }
    Ok(canonical)
}

/// # Errors
///
/// Returns an error if the projects cannot be loaded.
#[tauri::command]
pub async fn list_projects(db: State<'_, Db>) -> CommandResult<Vec<Project>> {
    let projects = db.list_projects().await?;
    Ok(projects)
}

/// # Errors
///
/// Returns an error if the query fails.
#[tauri::command]
pub async fn get_project(id: Uuid, db: State<'_, Db>) -> CommandResult<Option<Project>> {
    let project = db.get_project(id).await?;
    Ok(project)
}

/// # Errors
///
/// Returns an error if the input is invalid or the project cannot be stored.
#[tauri::command]
pub async fn create_project(
    input: CreateProjectInput,
    db: State<'_, Db>,
) -> CommandResult<Project> {
    let canonical_path = validate_project_path(&input.path)?;
    if let Some(existing) = db.get_project_by_path(&canonical_path).await? {
        return Ok(existing);
    }
    let mut input = input;
    input.path = canonical_path;
    let project = db.create_project(input).await?;
    record_activity_or_log(
        &db,
        "create_project activity",
        RecordActivityInput {
            project_id: Some(project.id),
            actor: Actor::User,
            action: ActionKind::Create,
            entity: EntityKind::Project,
            entity_id: Some(project.id),
            details: None,
        },
    )
    .await;
    Ok(project)
}

/// # Errors
///
/// Returns an error if the input is invalid, the project does not exist, or the update fails.
#[tauri::command]
pub async fn update_project(
    id: Uuid,
    input: UpdateProjectInput,
    db: State<'_, Db>,
) -> CommandResult<Project> {
    let is_notes_update =
        input.context_notes.is_some() && input.name.is_none() && input.status.is_none();
    let project = db.update_project(id, input).await?;
    let entity = if is_notes_update {
        EntityKind::ContextNotes
    } else {
        EntityKind::Project
    };
    record_activity_or_log(
        &db,
        "update_project activity",
        RecordActivityInput {
            project_id: Some(project.id),
            actor: Actor::User,
            action: ActionKind::Update,
            entity,
            entity_id: Some(project.id),
            details: None,
        },
    )
    .await;
    Ok(project)
}

/// # Errors
///
/// Returns an error if the project does not exist or cannot be deleted.
#[tauri::command]
pub async fn delete_project(id: Uuid, db: State<'_, Db>) -> CommandResult<()> {
    db.delete_project(id).await?;
    record_activity_or_log(
        &db,
        "delete_project activity",
        RecordActivityInput {
            project_id: None,
            actor: Actor::User,
            action: ActionKind::Delete,
            entity: EntityKind::Project,
            entity_id: Some(id),
            details: None,
        },
    )
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn canonicalize_path_handles_trailing_slash() {
        let temp = tempdir().unwrap();
        let canonical_expected = temp
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let path_with_slash = format!("{}/", temp.path().display());
        assert_eq!(canonicalize_path(&path_with_slash), canonical_expected);
    }

    #[test]
    fn canonicalize_path_handles_dot_segments() {
        let temp = tempdir().unwrap();
        let canonical_expected = temp
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let path_with_dots = format!("{}/./", temp.path().display());
        assert_eq!(canonicalize_path(&path_with_dots), canonical_expected);
    }

    #[test]
    fn canonicalize_path_handles_symlink_to_dir() {
        let temp = tempdir().unwrap();
        let target_dir = temp.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        let canonical_expected = target_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let link_path = temp.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target_dir, &link_path).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&target_dir, &link_path).unwrap();

        assert_eq!(
            canonicalize_path(&link_path.to_string_lossy()),
            canonical_expected
        );
    }

    #[test]
    fn canonicalize_path_handles_nonexistent_path() {
        let temp = tempdir().unwrap();
        let nonexistent = temp.path().join("nonexistent");
        let nonexistent_str = nonexistent.to_string_lossy().into_owned();
        assert_eq!(canonicalize_path(&nonexistent_str), nonexistent_str);
    }

    #[test]
    fn validate_project_path_rejects_nonexistent_and_files() {
        let temp = tempdir().unwrap();
        let nonexistent = temp.path().join("nonexistent");
        let result_nonexistent = validate_project_path(&nonexistent.to_string_lossy());
        assert!(matches!(result_nonexistent, Err(CoreError::Validation(_))));

        let file_path = temp.path().join("file.txt");
        fs::write(&file_path, "not a directory").unwrap();
        let result_file = validate_project_path(&file_path.to_string_lossy());
        assert!(matches!(result_file, Err(CoreError::Validation(_))));

        let result_dir = validate_project_path(&temp.path().to_string_lossy());
        assert!(result_dir.is_ok());
    }
}
