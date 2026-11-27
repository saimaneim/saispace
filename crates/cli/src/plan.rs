//! `sai plan`: el plan del proyecto en dos niveles (metas → tareas).

use crate::{CliError, CliResult};
use clap::Subcommand;
use saispace_core::models::{
    ActionKind, Actor, CreateTaskInput, EntityKind, GoalStatus, PlanFilter, RecordActivityInput,
    TaskStatus, UpdateGoalInput, UpdateTaskInput,
};
use saispace_core::{Db, default_db_path, short_id};

#[derive(Subcommand)]
pub enum PlanCommand {
    #[command(about = "Crea una meta (milestone) y, con --task, su desglose en tareas")]
    Goal {
        #[arg(help = "Título de la meta")]
        title: String,
        #[arg(help = "Cuándo se da por hecho", long)]
        criteria: Option<String>,
        #[arg(help = "Tarea del desglose (repetible)", long = "task", num_args = 0..)]
        tasks: Vec<String>,
    },
    #[command(about = "Crea una tarea, opcionalmente dentro de una meta")]
    Task {
        #[arg(help = "Título de la tarea")]
        title: String,
        #[arg(help = "Id de la meta (completo o los 8 últimos dígitos)", long)]
        goal: Option<String>,
    },
    #[command(about = "Marca una tarea como hecha")]
    Done {
        #[arg(help = "Id de la tarea (completo o los 8 últimos dígitos)")]
        id: String,
    },
    #[command(about = "Completa una meta")]
    Complete {
        #[arg(help = "Id de la meta (completo o los 8 últimos dígitos)")]
        id: String,
    },
}

fn actor() -> Actor {
    Actor::Agent {
        name: saispace_core::detect_agent_name("sai-cli"),
    }
}

fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "en curso",
        TaskStatus::Blocked => "bloqueada",
        TaskStatus::Done => "hecha",
        TaskStatus::Cancelled => "cancelada",
    }
}

/// # Errors
///
/// Returns an error if the database cannot be opened or a command fails.
pub fn run(command: Option<&PlanCommand>) -> CliResult<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async(command))
}

async fn run_async(command: Option<&PlanCommand>) -> CliResult<()> {
    let db_path = default_db_path()?;
    let db = Db::open(&db_path).await?;
    match command {
        None => show_plan(&db).await?,
        Some(PlanCommand::Goal {
            title,
            criteria,
            tasks,
        }) => create_goal(&db, title, criteria.clone(), tasks.clone()).await?,
        Some(PlanCommand::Task { title, goal }) => create_task(&db, title, goal.as_deref()).await?,
        Some(PlanCommand::Done { id }) => complete_task(&db, id).await?,
        Some(PlanCommand::Complete { id }) => complete_goal(&db, id).await?,
    }
    Ok(())
}

async fn show_plan(db: &Db) -> CliResult<()> {
    let project = crate::memory::current_project(db, false).await?;
    let Some(project) = project else {
        return Err(CliError::Usage(
            "este directorio no es un proyecto: entra en la carpeta del proyecto".to_string(),
        ));
    };
    let plan = db.get_plan(project.id, PlanFilter::Active).await?;
    if plan.goals.is_empty() && plan.loose_tasks.is_empty() {
        crate::out!("Sin plan activo en {}.", project.name);
        crate::out!("crea uno con `sai plan goal \"…\"` o `sai plan task \"…\"`");
        return Ok(());
    }
    crate::out!("# Plan de {}", project.name);
    for goal in &plan.goals {
        crate::out!(
            "\n## {} {}/{} {}",
            short_id(goal.goal.id),
            goal.done,
            goal.total,
            goal.goal.title
        );
        if let Some(criteria) = &goal.goal.criteria {
            crate::out!("   hecho cuando: {criteria}");
        }
        for task in &goal.tasks {
            crate::out!(
                "   - {} [{}] {}",
                short_id(task.id),
                status_label(task.status),
                task.title
            );
        }
    }
    if !plan.loose_tasks.is_empty() {
        crate::out!("\n## Tareas sueltas");
        for task in &plan.loose_tasks {
            crate::out!(
                "   - {} [{}] {}",
                short_id(task.id),
                status_label(task.status),
                task.title
            );
        }
    }
    Ok(())
}

async fn create_goal(
    db: &Db,
    title: &str,
    criteria: Option<String>,
    tasks: Vec<String>,
) -> CliResult<()> {
    let project = crate::memory::current_project(db, true).await?;
    let Some(project) = project else {
        return Err(CliError::Usage(
            "este directorio no es un proyecto (¿$HOME o ~/Dev?): entra en la carpeta del proyecto"
                .to_string(),
        ));
    };
    let created = db
        .create_goal_with_tasks(project.id, title.to_string(), criteria, &tasks)
        .await?;
    db.record_activity(RecordActivityInput {
        project_id: Some(project.id),
        actor: actor(),
        action: ActionKind::Create,
        entity: EntityKind::Goal,
        entity_id: Some(created.goal.id),
        details: Some(serde_json::json!({
            "title": created.goal.title,
            "tasks": created.tasks.len(),
        })),
    })
    .await?;
    crate::out!(
        "meta {} \"{}\" creada ({} {})",
        short_id(created.goal.id),
        created.goal.title,
        created.tasks.len(),
        if created.tasks.len() == 1 {
            "tarea"
        } else {
            "tareas"
        }
    );
    for task in &created.tasks {
        crate::out!("   - {} {}", short_id(task.id), task.title);
    }
    Ok(())
}

async fn create_task(db: &Db, title: &str, goal: Option<&str>) -> CliResult<()> {
    let (project_id, goal_id) = if let Some(raw) = goal {
        let goal_id = db.resolve_goal_id(raw).await?;
        let goal = db
            .get_goal(goal_id)
            .await?
            .ok_or(saispace_core::CoreError::NotFound {
                entity: "Goal",
                id: goal_id,
            })?;
        (goal.project_id, Some(goal_id))
    } else {
        let project = crate::memory::current_project(db, true).await?;
        let Some(project) = project else {
            return Err(CliError::Usage(
                "este directorio no es un proyecto (¿$HOME o ~/Dev?): entra en la carpeta del proyecto"
                    .to_string(),
            ));
        };
        (project.id, None)
    };
    let task = db
        .create_task(CreateTaskInput {
            project_id,
            title: title.to_string(),
            priority: None,
            goal_id,
        })
        .await?;
    db.record_activity(RecordActivityInput {
        project_id: Some(project_id),
        actor: actor(),
        action: ActionKind::Create,
        entity: EntityKind::Task,
        entity_id: Some(task.id),
        details: None,
    })
    .await?;
    if let Some(goal_id) = goal_id {
        crate::out!(
            "tarea {} \"{}\" creada (meta {})",
            short_id(task.id),
            task.title,
            short_id(goal_id)
        );
    } else {
        crate::out!("tarea {} \"{}\" creada", short_id(task.id), task.title);
    }
    Ok(())
}

async fn complete_task(db: &Db, raw: &str) -> CliResult<()> {
    let id = db.resolve_task_id(raw).await?;
    let updated = db
        .update_task(
            id,
            UpdateTaskInput {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .await?;
    db.record_activity(RecordActivityInput {
        project_id: Some(updated.task.project_id),
        actor: actor(),
        action: ActionKind::Update,
        entity: EntityKind::Task,
        entity_id: Some(updated.task.id),
        details: Some(serde_json::json!({ "status": "done" })),
    })
    .await?;
    crate::out!(
        "tarea {} \"{}\" completada",
        short_id(updated.task.id),
        updated.task.title
    );
    if let Some(progress) = updated.goal_progress
        && progress.goal_done
    {
        crate::out!(
            "¡todas las tareas de la meta {} están hechas! complétala con: sai plan complete {}",
            short_id(progress.goal_id),
            short_id(progress.goal_id)
        );
    }
    Ok(())
}

async fn complete_goal(db: &Db, raw: &str) -> CliResult<()> {
    let id = db.resolve_goal_id(raw).await?;
    let goal = db
        .update_goal(
            id,
            UpdateGoalInput {
                status: Some(GoalStatus::Completed),
                ..Default::default()
            },
        )
        .await?;
    db.record_activity(RecordActivityInput {
        project_id: Some(goal.project_id),
        actor: actor(),
        action: ActionKind::Update,
        entity: EntityKind::Goal,
        entity_id: Some(goal.id),
        details: Some(serde_json::json!({ "status": "completed" })),
    })
    .await?;
    crate::out!("meta {} \"{}\" completada", short_id(goal.id), goal.title);
    Ok(())
}
