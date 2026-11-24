use saispace_core::Db;
use saispace_core::models::{CreateProjectInput, MemoryKind, MemoryScope, SaveMemoryInput};

async fn setup() -> Db {
    let db = Db::connect_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    db
}

#[tokio::test]
async fn two_near_identical_titles_in_one_project_form_one_pair() {
    let db = setup().await;
    let project = db
        .create_project(CreateProjectInput {
            name: "test-proj".into(),
            path: "/p/test-proj".into(),
            context_notes: None,
        })
        .await
        .expect("project");

    let m1 = db
        .save_memory(SaveMemoryInput {
            project_id: Some(project.id),
            project_name: None,
            kind: MemoryKind::Config,
            title: "Database connection pooling configuration".to_string(),
            content: "Usar pool de 5 conexiones".to_string(),
            topic_key: None,
        })
        .await
        .expect("save m1");

    let m2 = db
        .save_memory(SaveMemoryInput {
            project_id: Some(project.id),
            project_name: None,
            kind: MemoryKind::Config,
            title: "Database connection pooling settings".to_string(),
            content: "Max 5 conexiones en sqlite".to_string(),
            topic_key: None,
        })
        .await
        .expect("save m2");

    let pairs = db
        .duplicate_pairs(MemoryScope::Project(project.id), 10)
        .await
        .expect("duplicate_pairs");

    assert_eq!(pairs.len(), 1);
    let pair = &pairs[0];
    let ids = [pair.a.id, pair.b.id];
    assert!(ids.contains(&m1.id));
    assert!(ids.contains(&m2.id));
}

#[tokio::test]
async fn same_titles_in_different_projects_do_not_pair() {
    let db = setup().await;
    let proj_a = db
        .create_project(CreateProjectInput {
            name: "proj-a".into(),
            path: "/p/proj-a".into(),
            context_notes: None,
        })
        .await
        .expect("project a");

    let proj_b = db
        .create_project(CreateProjectInput {
            name: "proj-b".into(),
            path: "/p/proj-b".into(),
            context_notes: None,
        })
        .await
        .expect("project b");

    db.save_memory(SaveMemoryInput {
        project_id: Some(proj_a.id),
        project_name: None,
        kind: MemoryKind::Architecture,
        title: "Microservices communication protocol".to_string(),
        content: "gRPC en servicios internos".to_string(),
        topic_key: None,
    })
    .await
    .expect("save a");

    db.save_memory(SaveMemoryInput {
        project_id: Some(proj_b.id),
        project_name: None,
        kind: MemoryKind::Architecture,
        title: "Microservices communication protocol".to_string(),
        content: "gRPC para backend".to_string(),
        topic_key: None,
    })
    .await
    .expect("save b");

    db.save_memory(SaveMemoryInput {
        project_id: None,
        project_name: None,
        kind: MemoryKind::Architecture,
        title: "Microservices communication protocol".to_string(),
        content: "gRPC global".to_string(),
        topic_key: None,
    })
    .await
    .expect("save global");

    let pairs = db
        .duplicate_pairs(MemoryScope::All, 10)
        .await
        .expect("duplicate_pairs");

    assert_eq!(pairs.len(), 0);
}

#[tokio::test]
async fn session_summaries_never_pair() {
    let db = setup().await;
    let project = db
        .create_project(CreateProjectInput {
            name: "proj-s".into(),
            path: "/p/proj-s".into(),
            context_notes: None,
        })
        .await
        .expect("project");

    db.save_memory(SaveMemoryInput {
        project_id: Some(project.id),
        project_name: None,
        kind: MemoryKind::SessionSummary,
        title: "Session summary sprint planning notes".to_string(),
        content: "Sesion 1".to_string(),
        topic_key: None,
    })
    .await
    .expect("save s1");

    db.save_memory(SaveMemoryInput {
        project_id: Some(project.id),
        project_name: None,
        kind: MemoryKind::SessionSummary,
        title: "Session summary sprint planning retrospective".to_string(),
        content: "Sesion 2".to_string(),
        topic_key: None,
    })
    .await
    .expect("save s2");

    let pairs = db
        .duplicate_pairs(MemoryScope::Project(project.id), 10)
        .await
        .expect("duplicate_pairs");

    assert_eq!(pairs.len(), 0);
}

#[tokio::test]
async fn symmetric_dedupe_returns_single_pair() {
    let db = setup().await;
    let project = db
        .create_project(CreateProjectInput {
            name: "proj-sym".into(),
            path: "/p/proj-sym".into(),
            context_notes: None,
        })
        .await
        .expect("project");

    let m1 = db
        .save_memory(SaveMemoryInput {
            project_id: Some(project.id),
            project_name: None,
            kind: MemoryKind::Pattern,
            title: "Repository pattern for data access layer".to_string(),
            content: "Implementar repositorios abstractos".to_string(),
            topic_key: None,
        })
        .await
        .expect("save m1");

    let m2 = db
        .save_memory(SaveMemoryInput {
            project_id: Some(project.id),
            project_name: None,
            kind: MemoryKind::Pattern,
            title: "Repository pattern for database access layer".to_string(),
            content: "Usar traits para repositorios".to_string(),
            topic_key: None,
        })
        .await
        .expect("save m2");

    let pairs = db
        .duplicate_pairs(MemoryScope::Project(project.id), 10)
        .await
        .expect("duplicate_pairs");

    assert_eq!(pairs.len(), 1);
    assert_ne!(pairs[0].a.id, pairs[0].b.id);
    let ids = [pairs[0].a.id, pairs[0].b.id];
    assert!(ids.contains(&m1.id));
    assert!(ids.contains(&m2.id));
}
