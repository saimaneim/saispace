use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::Db;
use saispace_core::models::{
    ActionKind, Actor, EntityKind, Memory, MemoryScope, RecordActivityInput,
};
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub async fn list_memories(
    project_id: Option<Uuid>,
    limit: Option<i64>,
    db: State<'_, Db>,
) -> CommandResult<Vec<Memory>> {
    Ok(db
        .recent_memories(
            project_id.map_or(MemoryScope::All, MemoryScope::Project),
            limit.unwrap_or(100),
        )
        .await?)
}

#[tauri::command]
pub async fn get_memory(id: Uuid, db: State<'_, Db>) -> CommandResult<Memory> {
    Ok(db.get_memory(id).await?)
}

#[tauri::command]
pub async fn delete_memory(id: Uuid, db: State<'_, Db>) -> CommandResult<()> {
    let memory = db.get_memory(id).await?;
    db.delete_memory(id).await?;
    record_activity_or_log(
        &db,
        "delete_memory activity",
        RecordActivityInput {
            project_id: memory.project_id,
            actor: Actor::User,
            action: ActionKind::Delete,
            entity: EntityKind::Memory,
            entity_id: Some(id),
            details: Some(serde_json::json!({ "title": memory.title })),
        },
    )
    .await;
    Ok(())
}
