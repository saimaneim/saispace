use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::Db;
use saispace_core::models::{
    ActionKind, Actor, CreateProjectInput, EntityKind, Project, RecordActivityInput,
    UpdateProjectInput,
};
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub async fn list_projects(db: State<'_, Db>) -> CommandResult<Vec<Project>> {
    let projects = db.list_projects().await?;
    Ok(projects)
}

#[tauri::command]
pub async fn get_project(id: Uuid, db: State<'_, Db>) -> CommandResult<Option<Project>> {
    let project = db.get_project(id).await?;
    Ok(project)
}

#[tauri::command]
pub async fn create_project(
    input: CreateProjectInput,
    db: State<'_, Db>,
) -> CommandResult<Project> {
    if let Some(existing) = db.get_project_by_path(input.path.trim()).await? {
        return Ok(existing);
    }
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
