use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::models::{
    ActionKind, Actor, EntityKind, RecordActivityInput, SecretGrant, SecretMeta,
};
use saispace_core::{Db, Vault};
use tauri::State;
use uuid::Uuid;

async fn record(db: &Db, action: ActionKind, details: serde_json::Value) {
    record_activity_or_log(
        db,
        "secret activity",
        RecordActivityInput {
            project_id: None,
            actor: Actor::User,
            action,
            entity: EntityKind::Secret,
            entity_id: None,
            details: Some(details),
        },
    )
    .await;
}

#[tauri::command]
pub async fn list_secrets(db: State<'_, Db>) -> CommandResult<Vec<SecretMeta>> {
    Ok(db.list_secrets().await?)
}

#[tauri::command]
pub async fn set_secret(
    name: String,
    value: String,
    description: Option<String>,
    db: State<'_, Db>,
    vault: State<'_, Vault>,
) -> CommandResult<SecretMeta> {
    let meta = db
        .set_secret(&vault, &name, &value, description.as_deref().unwrap_or(""))
        .await?;
    record(
        &db,
        ActionKind::Update,
        serde_json::json!({ "name": meta.name }),
    )
    .await;
    Ok(meta)
}

#[tauri::command]
pub async fn delete_secret(name: String, db: State<'_, Db>) -> CommandResult<()> {
    db.delete_secret(&name).await?;
    record(&db, ActionKind::Delete, serde_json::json!({ "name": name })).await;
    Ok(())
}

#[tauri::command]
pub async fn list_secret_grants(db: State<'_, Db>) -> CommandResult<Vec<SecretGrant>> {
    Ok(db.list_secret_grants().await?)
}

#[tauri::command]
pub async fn decide_secret_grant(id: Uuid, approve: bool, db: State<'_, Db>) -> CommandResult<()> {
    db.decide_secret_grant(id, approve).await?;
    record(
        &db,
        ActionKind::Update,
        serde_json::json!({ "grant": id, "approve": approve }),
    )
    .await;
    Ok(())
}

#[tauri::command]
pub async fn revoke_secret_grant(id: Uuid, db: State<'_, Db>) -> CommandResult<()> {
    db.revoke_secret_grant(id).await?;
    record(&db, ActionKind::Delete, serde_json::json!({ "grant": id })).await;
    Ok(())
}
