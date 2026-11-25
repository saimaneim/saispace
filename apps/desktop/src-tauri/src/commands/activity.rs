use crate::error::CommandResult;
use saispace_core::Db;
use saispace_core::models::ActivityEvent;
use tauri::State;
use uuid::Uuid;

/// # Errors
///
/// Returns an error if the activity rows cannot be loaded.
#[tauri::command]
pub async fn list_activity(
    limit: Option<i64>,
    db: State<'_, Db>,
) -> CommandResult<Vec<ActivityEvent>> {
    let limit = limit.unwrap_or(50);
    let activities = db.list_activity(limit).await?;
    Ok(activities)
}

/// # Errors
///
/// Returns an error if the activity rows cannot be loaded.
#[tauri::command]
pub async fn list_activity_by_project(
    project_id: Uuid,
    limit: Option<i64>,
    db: State<'_, Db>,
) -> CommandResult<Vec<ActivityEvent>> {
    let limit = limit.unwrap_or(50);
    let activities = db.list_activity_by_project(project_id, limit).await?;
    Ok(activities)
}
