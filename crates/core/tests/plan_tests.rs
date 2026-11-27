use saispace_core::models::{
    ActionKind, Actor, CreateGoalInput, CreateProjectInput, CreateTaskInput, EntityKind,
    GoalStatus, PlanFilter, RecordActivityInput, TaskPriority, TaskStatus, UpdateGoalInput,
    UpdateTaskInput,
};
use saispace_core::{CoreError, Db, short_id};
use uuid::Uuid;

async fn setup_test_db() -> Db {
    let db = Db::connect_in_memory()
        .await
        .expect("in-memory db connection should succeed");
    db.migrate().await.expect("migrations should succeed");
    db
}

async fn setup_project(db: &Db) -> Uuid {
    db.create_project(CreateProjectInput {
        name: "Plan Project".to_string(),
        path: "/plan/project".to_string(),
    })
    .await
    .expect("create_project failed")
    .id
}

#[tokio::test]
async fn test_goal_crud_and_status_transitions() {
    let db = setup_test_db().await;
    let project_id = setup_project(&db).await;

    let goal = db
        .create_goal(CreateGoalInput {
            project_id,
            title: "  Publicar la v0  ".to_string(),
            criteria: Some("  Desplegada y anunciada  ".to_string()),
            tasks: vec![],
        })
        .await
        .expect("create_goal failed");
    assert_eq!(goal.title, "Publicar la v0");
    assert_eq!(goal.criteria.as_deref(), Some("Desplegada y anunciada"));
    assert_eq!(goal.status, GoalStatus::Active);
    assert_eq!(goal.position, 0);
    assert!(goal.completed_at.is_none());

    let second = db
        .create_goal(CreateGoalInput {
            project_id,
            title: "Segundo".to_string(),
            criteria: None,
            tasks: vec![],
        })
        .await
        .expect("create_goal failed");
    assert_eq!(second.position, 1);

    let completed = db
        .update_goal(
            goal.id,
            UpdateGoalInput {
                status: Some(GoalStatus::Completed),
                ..Default::default()
            },
        )
        .await
        .expect("complete goal failed");
    assert_eq!(completed.status, GoalStatus::Completed);
    assert!(completed.completed_at.is_some());

    let reactivated = db
        .update_goal(
            goal.id,
            UpdateGoalInput {
                status: Some(GoalStatus::Active),
                ..Default::default()
            },
        )
        .await
        .expect("reactivate goal failed");
    assert_eq!(reactivated.status, GoalStatus::Active);
    assert!(reactivated.completed_at.is_none());

    let archived = db
        .update_goal(
            goal.id,
            UpdateGoalInput {
                status: Some(GoalStatus::Archived),
                ..Default::default()
            },
        )
        .await
        .expect("archive goal failed");
    assert_eq!(archived.status, GoalStatus::Archived);
    assert!(archived.completed_at.is_none());

    let missing = db
        .update_goal(
            Uuid::now_v7(),
            UpdateGoalInput {
                title: Some("x".to_string()),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(
        missing,
        Err(CoreError::NotFound { entity: "Goal", .. })
    ));

    db.delete_goal(goal.id).await.expect("delete_goal failed");
    assert!(db.get_goal(goal.id).await.expect("get").is_none());
    let delete_missing = db.delete_goal(goal.id).await;
    assert!(matches!(
        delete_missing,
        Err(CoreError::NotFound { entity: "Goal", .. })
    ));
}

#[tokio::test]
async fn test_create_goal_with_tasks_in_one_transaction() {
    let db = setup_test_db().await;
    let project_id = setup_project(&db).await;

    let created = db
        .create_goal_with_tasks(
            project_id,
            "Objetivo con desglose".to_string(),
            None,
            &["Primera tarea".to_string(), "Segunda tarea".to_string()],
        )
        .await
        .expect("create_goal_with_tasks failed");
    assert_eq!(created.tasks.len(), 2);
    assert_eq!(created.done, 0);
    assert_eq!(created.total, 2);
    assert!(
        created
            .tasks
            .iter()
            .all(|t| t.goal_id == Some(created.goal.id))
    );

    let invalid = db
        .create_goal_with_tasks(
            project_id,
            "Roto".to_string(),
            None,
            &["ok".to_string(), " ".to_string()],
        )
        .await;
    assert!(matches!(invalid, Err(CoreError::Validation(_))));
    // The transaction rolled back: no half-created goal remains.
    let plan = db
        .get_plan(project_id, PlanFilter::All)
        .await
        .expect("plan");
    assert_eq!(plan.counts.all, 1);
}

#[tokio::test]
async fn test_task_progress_and_loose_filtering() {
    let db = setup_test_db().await;
    let project_id = setup_project(&db).await;

    let goal = db
        .create_goal_with_tasks(
            project_id,
            "Objetivo".to_string(),
            Some("criterio".to_string()),
            &[
                "tarea 1".to_string(),
                "tarea 2".to_string(),
                "tarea 3".to_string(),
            ],
        )
        .await
        .expect("create");
    let task_ids: Vec<Uuid> = created_tasks(&db, project_id).await;
    assert_eq!(task_ids.len(), 3);

    // Loose open task and a loose done task (done loose tasks are not listed).
    let loose = db
        .create_task(CreateTaskInput {
            project_id,
            title: "tarea suelta".to_string(),
            priority: None,
            goal_id: None,
        })
        .await
        .expect("loose task");
    let loose_done = db
        .create_task(CreateTaskInput {
            project_id,
            title: "suelta hecha".to_string(),
            priority: None,
            goal_id: None,
        })
        .await
        .expect("loose done task");
    db.update_task(
        loose_done.id,
        UpdateTaskInput {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .await
    .expect("done loose");

    // Progress after completing one task.
    let updated = db
        .update_task(
            task_ids[0],
            UpdateTaskInput {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .await
        .expect("done task");
    let progress = updated.goal_progress.expect("goal progress");
    assert_eq!(progress.goal_id, goal.goal.id);
    assert_eq!(progress.done, 1);
    assert_eq!(progress.total, 3);
    assert!(!progress.goal_done);

    // Cancelled tasks leave the total.
    db.update_task(
        task_ids[1],
        UpdateTaskInput {
            status: Some(TaskStatus::Cancelled),
            ..Default::default()
        },
    )
    .await
    .expect("cancel task");
    let touched = db
        .update_task(
            task_ids[2],
            UpdateTaskInput {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .await
        .expect("done last task");
    let progress = touched.goal_progress.expect("goal progress");
    assert_eq!(progress.done, 2);
    assert_eq!(progress.total, 2);
    assert!(progress.goal_done);

    // Active plan: goal with its tasks open-first, loose tasks, counts.
    let plan = db
        .get_plan(project_id, PlanFilter::Active)
        .await
        .expect("plan");
    assert_eq!(plan.counts.active, 1);
    assert_eq!(plan.counts.completed, 0);
    assert_eq!(plan.counts.all, 1);
    assert_eq!(plan.goals.len(), 1);
    assert_eq!(plan.goals[0].tasks.len(), 3);
    // Closed first (by creation), open later: tarea 1 done, tarea 2 cancelled.
    assert_eq!(plan.goals[0].tasks[0].status, TaskStatus::Done);
    assert_eq!(plan.goals[0].tasks[1].status, TaskStatus::Cancelled);
    assert_eq!(plan.loose_tasks.len(), 1);
    assert_eq!(plan.loose_tasks[0].id, loose.id);
}

#[tokio::test]
async fn test_plan_filters_and_ordering() {
    let db = setup_test_db().await;
    let project_id = setup_project(&db).await;

    let goal = db
        .create_goal_with_tasks(
            project_id,
            "Objetivo".to_string(),
            None,
            &["tarea".to_string()],
        )
        .await
        .expect("create");

    // Completed filter shows the goal after completing it.
    db.update_goal(
        goal.goal.id,
        UpdateGoalInput {
            status: Some(GoalStatus::Completed),
            ..Default::default()
        },
    )
    .await
    .expect("complete goal");
    let active_plan = db
        .get_plan(project_id, PlanFilter::Active)
        .await
        .expect("plan");
    assert_eq!(active_plan.goals, Vec::new());
    assert_eq!(active_plan.counts.completed, 1);
    let completed_plan = db
        .get_plan(project_id, PlanFilter::Completed)
        .await
        .expect("plan");
    assert_eq!(completed_plan.goals.len(), 1);

    // Tasks are ordered open first within a goal.
    db.update_task(
        created_tasks(&db, project_id).await[0],
        UpdateTaskInput {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .await
    .expect("close original task");
    db.create_task(CreateTaskInput {
        project_id,
        title: "abierta".to_string(),
        priority: None,
        goal_id: Some(goal.goal.id),
    })
    .await
    .expect("open task");
    let completed_plan = db
        .get_plan(project_id, PlanFilter::Completed)
        .await
        .expect("plan");
    let tasks = &completed_plan.goals[0].tasks;
    assert_eq!(tasks[0].title, "abierta");
}

#[tokio::test]
async fn test_delete_goal_keeps_tasks_loose() {
    let db = setup_test_db().await;
    let project_id = setup_project(&db).await;

    let goal = db
        .create_goal_with_tasks(
            project_id,
            "A borrar".to_string(),
            None,
            &["paso".to_string()],
        )
        .await
        .expect("create");
    let task_id = created_tasks(&db, project_id).await[0];

    db.delete_goal(goal.goal.id).await.expect("delete goal");
    let task = db
        .get_task(task_id)
        .await
        .expect("get")
        .expect("task survives");
    assert_eq!(task.goal_id, None);

    // The surviving task is loose and open now.
    let plan = db
        .get_plan(project_id, PlanFilter::All)
        .await
        .expect("plan");
    assert_eq!(plan.loose_tasks.len(), 1);
    assert_eq!(plan.loose_tasks[0].id, task_id);
}

#[tokio::test]
async fn test_resolve_goal_and_task_short_ids() {
    let db = setup_test_db().await;
    let project_id = setup_project(&db).await;

    let goal = db
        .create_goal(CreateGoalInput {
            project_id,
            title: "Resolver".to_string(),
            criteria: None,
            tasks: vec![],
        })
        .await
        .expect("goal");
    let task = db
        .create_task(CreateTaskInput {
            project_id,
            title: "t".to_string(),
            priority: Some(TaskPriority::High),
            goal_id: Some(goal.id),
        })
        .await
        .expect("task");

    let goal_short = short_id(goal.id);
    let task_short = short_id(task.id);
    assert_eq!(
        db.resolve_goal_id(&goal_short).await.expect("resolve"),
        goal.id
    );
    assert_eq!(
        db.resolve_goal_id(&goal.id.to_string())
            .await
            .expect("resolve"),
        goal.id
    );
    assert_eq!(
        db.resolve_task_id(&task_short).await.expect("resolve"),
        task.id
    );

    let missing = db.resolve_goal_id("000000000000").await;
    assert!(matches!(missing, Err(CoreError::Validation(_))));
    let not_hex = db.resolve_goal_id("xyz").await;
    assert!(matches!(not_hex, Err(CoreError::Validation(_))));
}

#[tokio::test]
async fn test_create_task_validates_goal_and_activity_kinds() {
    let db = setup_test_db().await;
    let project_id = setup_project(&db).await;

    let missing_goal = db
        .create_task(CreateTaskInput {
            project_id,
            title: "sin objetivo".to_string(),
            priority: None,
            goal_id: Some(Uuid::now_v7()),
        })
        .await;
    assert!(matches!(
        missing_goal,
        Err(CoreError::NotFound { entity: "Goal", .. })
    ));

    let goal = db
        .create_goal(CreateGoalInput {
            project_id,
            title: "Con actividad".to_string(),
            criteria: None,
            tasks: vec![],
        })
        .await
        .expect("goal");
    db.record_activity(RecordActivityInput {
        project_id: Some(project_id),
        actor: Actor::Agent {
            name: "test".to_string(),
        },
        action: ActionKind::Create,
        entity: EntityKind::Goal,
        entity_id: Some(goal.id),
        details: None,
    })
    .await
    .expect("record");

    let events = db.list_activity(10).await.expect("activity");
    assert!(events.iter().any(|e| {
        e.action == ActionKind::Create
            && e.entity == EntityKind::Goal
            && e.entity_id == Some(goal.id)
    }));
}

async fn created_tasks(db: &Db, project_id: Uuid) -> Vec<Uuid> {
    db.list_tasks_by_project(project_id)
        .await
        .expect("list tasks")
        .into_iter()
        .map(|t| t.id)
        .collect()
}
