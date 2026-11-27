use crate::error::CommandResult;
use saispace_core::Db;
use saispace_core::models::TimelineDay;
use tauri::State;
use uuid::Uuid;

/// The project's history grouped by local calendar day.
///
/// # Errors
///
/// Returns an error if the project does not exist or its plan cannot be read.
#[tauri::command]
pub async fn project_timeline(
    project_id: Uuid,
    db: State<'_, Db>,
) -> CommandResult<Vec<TimelineDay>> {
    Ok(db.project_timeline(project_id, &chrono::Local).await?)
}
