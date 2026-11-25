use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::Db;
use saispace_core::models::{
    ActionKind, Actor, CreateTaskInput, EntityKind, RecordActivityInput, Task, UpdateTaskInput,
};
use tauri::State;
use uuid::Uuid;

/// # Errors
///
/// Returns an error if the tasks cannot be loaded.
#[tauri::command]
pub async fn list_tasks(project_id: Uuid, db: State<'_, Db>) -> CommandResult<Vec<Task>> {
    let tasks = db.list_tasks_by_project(project_id).await?;
    Ok(tasks)
}

/// # Errors
///
/// Returns an error if the input is invalid or the task cannot be stored.
#[tauri::command]
pub async fn create_task(input: CreateTaskInput, db: State<'_, Db>) -> CommandResult<Task> {
    let project_id = input.project_id;
    let task = db.create_task(input).await?;
    record_activity_or_log(
        &db,
        "create_task activity",
        RecordActivityInput {
            project_id: Some(project_id),
            actor: Actor::User,
            action: ActionKind::Create,
            entity: EntityKind::Task,
            entity_id: Some(task.id),
            details: None,
        },
    )
    .await;
    Ok(task)
}

/// # Errors
///
/// Returns an error if the input is invalid, the task does not exist, or the update fails.
#[tauri::command]
pub async fn update_task(
    id: Uuid,
    input: UpdateTaskInput,
    db: State<'_, Db>,
) -> CommandResult<Task> {
    let task = db.update_task(id, input).await?;
    record_activity_or_log(
        &db,
        "update_task activity",
        RecordActivityInput {
            project_id: Some(task.project_id),
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::Task,
            entity_id: Some(task.id),
            details: None,
        },
    )
    .await;
    Ok(task)
}

/// # Errors
///
/// Returns an error if the task does not exist or cannot be deleted.
#[tauri::command]
pub async fn delete_task(id: Uuid, db: State<'_, Db>) -> CommandResult<()> {
    let existing = db.get_task(id).await?;
    let project_id = existing.map(|t| t.project_id);
    db.delete_task(id).await?;
    record_activity_or_log(
        &db,
        "delete_task activity",
        RecordActivityInput {
            project_id,
            actor: Actor::User,
            action: ActionKind::Delete,
            entity: EntityKind::Task,
            entity_id: Some(id),
            details: None,
        },
    )
    .await;
    Ok(())
}
