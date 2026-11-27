use crate::error::CommandResult;
use crate::error_log::record_activity_or_log;
use saispace_core::Db;
use saispace_core::models::{
    ActionKind, Actor, EntityKind, Goal, GoalStatus, GoalWithTasks, Plan, PlanFilter,
    RecordActivityInput, UpdateGoalInput,
};
use tauri::State;
use uuid::Uuid;

/// # Errors
///
/// Returns an error if the plan cannot be loaded.
#[tauri::command]
pub async fn plan_get(
    project_id: Uuid,
    filter: PlanFilter,
    db: State<'_, Db>,
) -> CommandResult<Plan> {
    let plan = db.get_plan(project_id, filter).await?;
    Ok(plan)
}

/// Creates a goal and its task breakdown in one transaction.
///
/// # Errors
///
/// Returns an error if any input is invalid or the goal cannot be stored.
#[tauri::command]
pub async fn goal_create(
    project_id: Uuid,
    title: String,
    criteria: Option<String>,
    tasks: Vec<String>,
    db: State<'_, Db>,
) -> CommandResult<GoalWithTasks> {
    let created = db
        .create_goal_with_tasks(project_id, title, criteria, &tasks)
        .await?;
    record_activity_or_log(
        &db,
        "goal_create activity",
        RecordActivityInput {
            project_id: Some(project_id),
            actor: Actor::User,
            action: ActionKind::Create,
            entity: EntityKind::Goal,
            entity_id: Some(created.goal.id),
            details: None,
        },
    )
    .await;
    Ok(created)
}

/// # Errors
///
/// Returns an error if the input is invalid, the goal does not exist, or the
/// update fails.
#[tauri::command]
pub async fn goal_update(
    id: Uuid,
    input: UpdateGoalInput,
    db: State<'_, Db>,
) -> CommandResult<Goal> {
    let completed = input.status == Some(GoalStatus::Completed);
    let goal = db.update_goal(id, input).await?;
    record_activity_or_log(
        &db,
        "goal_update activity",
        RecordActivityInput {
            project_id: Some(goal.project_id),
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::Goal,
            entity_id: Some(goal.id),
            details: completed.then(|| serde_json::json!({ "status": "completed" })),
        },
    )
    .await;
    Ok(goal)
}

/// Deletes a goal; its tasks keep existing with `goal_id` null.
///
/// # Errors
///
/// Returns an error if the goal does not exist or the delete fails.
#[tauri::command]
pub async fn goal_delete(id: Uuid, db: State<'_, Db>) -> CommandResult<()> {
    let existing = db.get_goal(id).await?;
    let project_id = existing.map(|g| g.project_id);
    db.delete_goal(id).await?;
    record_activity_or_log(
        &db,
        "goal_delete activity",
        RecordActivityInput {
            project_id,
            actor: Actor::User,
            action: ActionKind::Delete,
            entity: EntityKind::Goal,
            entity_id: Some(id),
            details: None,
        },
    )
    .await;
    Ok(())
}
