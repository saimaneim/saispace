use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::Db;
use saispace_core::models::{
    ActionKind, Actor, EntityKind, Memory, MemoryScope, MemoryUsageStats, RecordActivityInput,
    UpdateMemoryInput,
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

#[tauri::command]
pub async fn update_memory(
    id: Uuid,
    input: UpdateMemoryInput,
    db: State<'_, Db>,
) -> CommandResult<Memory> {
    let memory = db.update_memory(id, input).await?;
    record_activity_or_log(
        &db,
        "update_memory activity",
        RecordActivityInput {
            project_id: memory.project_id,
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::Memory,
            entity_id: Some(id),
            details: Some(serde_json::json!({ "title": memory.title })),
        },
    )
    .await;
    Ok(memory)
}

#[tauri::command]
pub async fn set_memory_pinned(id: Uuid, pinned: bool, db: State<'_, Db>) -> CommandResult<Memory> {
    let memory = db.set_memory_pinned(id, pinned).await?;
    record_activity_or_log(
        &db,
        "set_memory_pinned activity",
        RecordActivityInput {
            project_id: memory.project_id,
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::Memory,
            entity_id: Some(id),
            details: Some(serde_json::json!({ "title": memory.title, "pinned": pinned })),
        },
    )
    .await;
    Ok(memory)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentContext {
    pub text: String,
    pub bytes: usize,
}

#[tauri::command]
pub async fn agent_memory_context(
    project_id: Option<Uuid>,
    db: State<'_, Db>,
) -> CommandResult<AgentContext> {
    let project = match project_id {
        Some(id) => db.get_project(id).await?,
        None => None,
    };
    let scope = MemoryScope::for_project(project_id);
    let context = db.memory_context(scope, 12).await?;
    let text = saispace_core::memory_render::render_context(project.as_ref(), &context);
    let bytes = text.len();
    Ok(AgentContext { text, bytes })
}

#[tauri::command]
pub async fn list_orphan_memory_groups(
    db: State<'_, Db>,
) -> CommandResult<Vec<saispace_core::db::OrphanGroup>> {
    Ok(db.orphan_memory_groups().await?)
}

#[tauri::command]
pub async fn link_orphan_memories(
    project_name: String,
    project_id: Uuid,
    db: State<'_, Db>,
) -> CommandResult<u64> {
    let linked = db
        .link_orphans_to_project(&project_name, project_id)
        .await?;
    record_activity_or_log(
        &db,
        "link_orphan_memories activity",
        RecordActivityInput {
            project_id: Some(project_id),
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::Memory,
            entity_id: None,
            details: Some(serde_json::json!({
                "project_name": project_name,
                "linked": linked,
            })),
        },
    )
    .await;
    Ok(linked)
}

#[tauri::command]
pub async fn memory_duplicates(
    project_id: Option<Uuid>,
    db: State<'_, Db>,
) -> CommandResult<Vec<saispace_core::DuplicatePair>> {
    let scope = project_id.map_or(MemoryScope::All, MemoryScope::Project);
    Ok(db.duplicate_pairs(scope, 50).await?)
}

#[tauri::command]
pub async fn memory_usage_stats(db: State<'_, Db>) -> CommandResult<MemoryUsageStats> {
    Ok(db.memory_usage_stats().await?)
}
