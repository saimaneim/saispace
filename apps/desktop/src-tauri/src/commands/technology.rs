use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::models::{
    ActionKind, Actor, EntityKind, RecordActivityInput, SearchHit, TechRule, Technology,
    UpsertTechRuleInput,
};
use saispace_core::{CoreError, Db};
use tauri::State;
use uuid::Uuid;

/// # Errors
///
/// Returns an error if the technologies cannot be loaded.
#[tauri::command]
pub async fn list_technologies(
    project_id: Uuid,
    db: State<'_, Db>,
) -> CommandResult<Vec<Technology>> {
    Ok(db.list_technologies(project_id).await?)
}

/// # Errors
///
/// Returns an error if the project does not exist or its stack cannot be synced.
#[tauri::command]
pub async fn sync_project_stack(
    project_id: Uuid,
    db: State<'_, Db>,
) -> CommandResult<Vec<Technology>> {
    let stored = db.sync_project_stack(project_id).await?;
    record_activity_or_log(
        &db,
        "sync_project_stack activity",
        RecordActivityInput {
            project_id: Some(project_id),
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::Stack,
            entity_id: Some(project_id),
            details: Some(serde_json::json!({ "technologies": stored.len() })),
        },
    )
    .await;
    Ok(stored)
}

/// # Errors
///
/// Returns an error if the rules cannot be loaded.
#[tauri::command]
pub async fn list_tech_rules(
    technology: Option<String>,
    db: State<'_, Db>,
) -> CommandResult<Vec<TechRule>> {
    Ok(db.list_tech_rules(technology.as_deref()).await?)
}

/// # Errors
///
/// Returns an error if the rules cannot be loaded.
#[tauri::command]
pub async fn list_project_tech_rules(
    project_id: Uuid,
    db: State<'_, Db>,
) -> CommandResult<Vec<TechRule>> {
    Ok(db.tech_rules_for_project(project_id).await?)
}

/// # Errors
///
/// Returns an error if the input is invalid or the rule cannot be stored.
#[tauri::command]
pub async fn upsert_tech_rule(
    input: UpsertTechRuleInput,
    db: State<'_, Db>,
) -> CommandResult<TechRule> {
    let saved = db.upsert_tech_rule(input).await?;
    record_activity_or_log(
        &db,
        "upsert_tech_rule activity",
        RecordActivityInput {
            project_id: None,
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::TechRule,
            entity_id: Some(saved.id),
            details: Some(serde_json::json!({
                "technology": saved.technology,
                "version": saved.version_req,
            })),
        },
    )
    .await;
    Ok(saved)
}

/// # Errors
///
/// Returns an error if the rule does not exist or cannot be deleted.
#[tauri::command]
pub async fn delete_tech_rule(id: Uuid, db: State<'_, Db>) -> CommandResult<()> {
    db.delete_tech_rule(id).await?;
    record_activity_or_log(
        &db,
        "delete_tech_rule activity",
        RecordActivityInput {
            project_id: None,
            actor: Actor::User,
            action: ActionKind::Delete,
            entity: EntityKind::TechRule,
            entity_id: Some(id),
            details: None,
        },
    )
    .await;
    Ok(())
}

/// # Errors
///
/// Returns an error if the search query fails.
#[tauri::command]
pub async fn search_memory(
    query: String,
    project_id: Option<Uuid>,
    limit: Option<i64>,
    db: State<'_, Db>,
) -> CommandResult<Vec<SearchHit>> {
    Ok(db.search(&query, project_id, limit).await?)
}

/// # Errors
///
/// Returns an error if the URL is not http(s) or the browser cannot be opened.
#[tauri::command]
pub fn open_url(url: String) -> CommandResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(CoreError::Validation("only http(s) urls can be opened".into()).into());
    }
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };
    command.arg(url).spawn()?;
    Ok(())
}
