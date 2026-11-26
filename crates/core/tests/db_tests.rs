use saispace_core::models::{
    ActionKind, Actor, CreateProjectInput, CreateTaskInput, EntityKind, ProjectStatus,
    RecordActivityInput, TaskPriority, TaskStatus, UpdateProjectInput, UpdateTaskInput,
};
use saispace_core::{CoreError, Db};
use serde_json::json;
use uuid::Uuid;

async fn setup_test_db() -> Db {
    let db = Db::connect_in_memory()
        .await
        .expect("in-memory db connection should succeed");
    db.migrate().await.expect("migrations should succeed");
    db
}

#[tokio::test]
async fn test_serde_defaults() {
    let json_project = r#"{"name":"Test","path":"/tmp/test"}"#;
    let input: CreateProjectInput =
        serde_json::from_str(json_project).expect("should deserialize name and path");
    assert_eq!(input.name, "Test");

    let json_task = format!(r#"{{"project_id":"{}","title":"Task"}}"#, Uuid::new_v4());
    let task_input: CreateTaskInput =
        serde_json::from_str(&json_task).expect("should deserialize without priority");
    assert_eq!(task_input.priority, None);
}

#[tokio::test]
async fn test_project_lifecycle() {
    let db = setup_test_db().await;

    let created = db
        .create_project(CreateProjectInput {
            name: "Saispace".to_string(),
            path: "/path/to/saispace".to_string(),
        })
        .await
        .expect("create_project failed");

    assert_eq!(created.name, "Saispace");
    assert_eq!(created.path, "/path/to/saispace");
    assert_eq!(created.status, ProjectStatus::Active);

    let by_id = db
        .get_project(created.id)
        .await
        .expect("get_project failed")
        .expect("project not found by id");
    assert_eq!(by_id.id, created.id);

    let by_path = db
        .get_project_by_path("/path/to/saispace")
        .await
        .expect("get_project_by_path failed")
        .expect("project not found by path");
    assert_eq!(by_path.id, created.id);

    let duplicate_path = db
        .create_project(CreateProjectInput {
            name: "Duplicate Path".to_string(),
            path: "/path/to/saispace".to_string(),
        })
        .await;
    assert!(duplicate_path.is_err());

    let updated = db
        .update_project(
            created.id,
            UpdateProjectInput {
                name: Some("Saispace HQ".to_string()),
                path: None,
                status: Some(ProjectStatus::OnHold),
            },
        )
        .await
        .expect("update_project failed");
    assert_eq!(updated.name, "Saispace HQ");
    assert_eq!(updated.status, ProjectStatus::OnHold);

    let projects = db.list_projects().await.expect("list_projects failed");
    assert_eq!(projects.len(), 1);

    db.delete_project(created.id)
        .await
        .expect("delete_project failed");

    let deleted_check = db
        .get_project(created.id)
        .await
        .expect("get_project after delete failed");
    assert!(deleted_check.is_none());

    let delete_non_existent = db.delete_project(created.id).await;
    assert!(matches!(
        delete_non_existent,
        Err(CoreError::NotFound {
            entity: "Project",
            ..
        })
    ));
}

#[tokio::test]
async fn test_task_lifecycle_and_cascade() {
    let db = setup_test_db().await;

    let project = db
        .create_project(CreateProjectInput {
            name: "Project A".to_string(),
            path: "/projects/a".to_string(),
        })
        .await
        .expect("create_project failed");

    let task = db
        .create_task(CreateTaskInput {
            project_id: project.id,
            title: "First Task".to_string(),
            priority: Some(TaskPriority::Urgent),
        })
        .await
        .expect("create_task failed");

    assert_eq!(task.project_id, project.id);
    assert_eq!(task.title, "First Task");
    assert_eq!(task.status, TaskStatus::Todo);
    assert_eq!(task.priority, TaskPriority::Urgent);
    assert!(task.completed_at.is_none());

    let updated_done = db
        .update_task(
            task.id,
            UpdateTaskInput {
                title: None,
                status: Some(TaskStatus::Done),
                priority: None,
            },
        )
        .await
        .expect("update_task to Done failed");
    assert_eq!(updated_done.status, TaskStatus::Done);
    assert!(updated_done.completed_at.is_some());

    let updated_reopened = db
        .update_task(
            task.id,
            UpdateTaskInput {
                title: None,
                status: Some(TaskStatus::InProgress),
                priority: None,
            },
        )
        .await
        .expect("update_task to InProgress failed");
    assert_eq!(updated_reopened.status, TaskStatus::InProgress);
    assert!(updated_reopened.completed_at.is_none());

    let tasks = db
        .list_tasks_by_project(project.id)
        .await
        .expect("list_tasks_by_project failed");
    assert_eq!(tasks.len(), 1);

    db.delete_project(project.id)
        .await
        .expect("delete_project failed");

    let task_after_cascade = db.get_task(task.id).await.expect("get_task check failed");
    assert!(task_after_cascade.is_none());
}

#[tokio::test]
async fn test_foreign_key_violation() {
    let db = setup_test_db().await;

    let non_existent_project_id = Uuid::now_v7();
    let result = db
        .create_task(CreateTaskInput {
            project_id: non_existent_project_id,
            title: "Orphan Task".to_string(),
            priority: None,
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_activity_recording_and_filtering() {
    let db = setup_test_db().await;

    let project = db
        .create_project(CreateProjectInput {
            name: "Project Beta".to_string(),
            path: "/projects/beta".to_string(),
        })
        .await
        .expect("create_project failed");

    let event_agent = db
        .record_activity(RecordActivityInput {
            project_id: Some(project.id),
            actor: Actor::Agent {
                name: "Claude".to_string(),
            },
            action: ActionKind::Create,
            entity: EntityKind::Project,
            entity_id: Some(project.id),
            details: Some(json!({ "branch": "main" })),
        })
        .await
        .expect("record_activity agent failed");

    assert_eq!(
        event_agent.actor,
        Actor::Agent {
            name: "Claude".to_string()
        }
    );
    assert_eq!(event_agent.action, ActionKind::Create);
    assert_eq!(event_agent.entity, EntityKind::Project);
    assert_eq!(event_agent.project_id, Some(project.id));
    assert_eq!(event_agent.details, Some(json!({ "branch": "main" })));

    let event_user = db
        .record_activity(RecordActivityInput {
            project_id: Some(project.id),
            actor: Actor::User,
            action: ActionKind::Update,
            entity: EntityKind::ContextNotes,
            entity_id: Some(project.id),
            details: None,
        })
        .await
        .expect("record_activity user failed");

    assert_eq!(event_user.actor, Actor::User);
    assert_eq!(event_user.action, ActionKind::Update);

    let all_events = db.list_activity(50).await.expect("list_activity failed");
    assert_eq!(all_events.len(), 2);

    let project_events = db
        .list_activity_by_project(project.id, 50)
        .await
        .expect("list_activity_by_project failed");
    assert_eq!(project_events.len(), 2);

    let context_events = db
        .list_activity_by_entity(EntityKind::ContextNotes, project.id, 50)
        .await
        .expect("list_activity_by_entity failed");
    assert_eq!(context_events.len(), 1);
    assert_eq!(context_events[0].id, event_user.id);
}

fn project_input(name: &str, path: &str) -> CreateProjectInput {
    CreateProjectInput {
        name: name.to_string(),
        path: path.to_string(),
    }
}

#[tokio::test]
async fn test_create_project_rejects_blank_name_and_path() {
    let db = setup_test_db().await;

    let blank_name = db
        .create_project(project_input("   ", "/validation/a"))
        .await;
    assert!(matches!(blank_name, Err(CoreError::Validation(_))));

    let blank_path = db.create_project(project_input("Name", " \t ")).await;
    assert!(matches!(blank_path, Err(CoreError::Validation(_))));

    let empty_name = db.create_project(project_input("", "/validation/b")).await;
    assert!(matches!(empty_name, Err(CoreError::Validation(_))));
}

#[tokio::test]
async fn test_create_project_enforces_name_length_limit() {
    let db = setup_test_db().await;

    let at_limit = "a".repeat(200);
    let accepted = db
        .create_project(project_input(&at_limit, "/validation/limit-ok"))
        .await
        .expect("200 chars should be accepted");
    assert_eq!(accepted.name.chars().count(), 200);

    let over_limit = "a".repeat(201);
    let rejected = db
        .create_project(project_input(&over_limit, "/validation/limit-over"))
        .await;
    assert!(matches!(rejected, Err(CoreError::Validation(_))));
}

#[tokio::test]
async fn test_create_project_trims_name_and_path() {
    let db = setup_test_db().await;

    let created = db
        .create_project(project_input("  Spaced  ", "  /validation/trim  "))
        .await
        .expect("create_project failed");
    assert_eq!(created.name, "Spaced");
    assert_eq!(created.path, "/validation/trim");

    let by_path = db
        .get_project_by_path("/validation/trim")
        .await
        .expect("get_project_by_path failed");
    assert!(by_path.is_some());
}

#[tokio::test]
async fn test_update_project_validates_name_and_path() {
    let db = setup_test_db().await;
    let project = db
        .create_project(project_input("Original", "/validation/update"))
        .await
        .expect("create_project failed");

    let blank_name = db
        .update_project(
            project.id,
            UpdateProjectInput {
                name: Some("  ".to_string()),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(blank_name, Err(CoreError::Validation(_))));

    let blank_path = db
        .update_project(
            project.id,
            UpdateProjectInput {
                path: Some(String::new()),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(blank_path, Err(CoreError::Validation(_))));

    let too_long = db
        .update_project(
            project.id,
            UpdateProjectInput {
                name: Some("b".repeat(201)),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(too_long, Err(CoreError::Validation(_))));

    let unchanged = db
        .get_project(project.id)
        .await
        .expect("get_project failed")
        .expect("project should still exist");
    assert_eq!(unchanged.name, "Original");
    assert_eq!(unchanged.path, "/validation/update");

    let trimmed = db
        .update_project(
            project.id,
            UpdateProjectInput {
                name: Some("  Renamed  ".to_string()),
                path: Some(" /validation/renamed ".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("valid update should succeed");
    assert_eq!(trimmed.name, "Renamed");
    assert_eq!(trimmed.path, "/validation/renamed");
}

#[tokio::test]
async fn test_create_and_update_task_validate_title() {
    let db = setup_test_db().await;
    let project = db
        .create_project(project_input("Tasks", "/validation/tasks"))
        .await
        .expect("create_project failed");

    let blank = db
        .create_task(CreateTaskInput {
            project_id: project.id,
            title: "   ".to_string(),
            priority: None,
        })
        .await;
    assert!(matches!(blank, Err(CoreError::Validation(_))));

    let too_long = db
        .create_task(CreateTaskInput {
            project_id: project.id,
            title: "t".repeat(201),
            priority: None,
        })
        .await;
    assert!(matches!(too_long, Err(CoreError::Validation(_))));

    let task = db
        .create_task(CreateTaskInput {
            project_id: project.id,
            title: "  Ship it  ".to_string(),
            priority: None,
        })
        .await
        .expect("create_task failed");
    assert_eq!(task.title, "Ship it");

    let blank_update = db
        .update_task(
            task.id,
            UpdateTaskInput {
                title: Some(" ".to_string()),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(blank_update, Err(CoreError::Validation(_))));

    let renamed = db
        .update_task(
            task.id,
            UpdateTaskInput {
                title: Some("  Renamed task  ".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("valid update should succeed");
    assert_eq!(renamed.title, "Renamed task");
}
