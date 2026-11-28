use saispace_core::Db;
use saispace_core::db::sync::SYNC_DIR;
use saispace_core::models::{
    CreateProjectInput, MemoryKind, MemoryRelation, Project, SaveMemoryInput, Visibility,
};

async fn db_with_project(dir: &std::path::Path, file: &str) -> (Db, Project) {
    let db = Db::open(&dir.join(file)).await.expect("db");
    let project = db
        .create_project(CreateProjectInput {
            name: "tienda".into(),
            path: dir.join("repo").display().to_string(),
        })
        .await
        .expect("project");
    (db, project)
}

fn save(project: Option<&Project>, title: &str, topic: Option<&str>) -> SaveMemoryInput {
    SaveMemoryInput {
        project_id: project.map(|p| p.id),
        project_name: None,
        kind: MemoryKind::Decision,
        title: title.into(),
        content: format!("Contenido de {title}"),
        topic_key: topic.map(String::from),
    }
}

#[tokio::test]
async fn export_import_roundtrip_is_idempotent_and_keeps_private_data_home() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (a, pa) = db_with_project(tmp.path(), "a.db").await;
    let shared = a
        .save_memory(save(Some(&pa), "Usar WAL", Some("arch/db")))
        .await
        .expect("shared");
    let other = a
        .save_memory(save(Some(&pa), "Pool de 5 conexiones", None))
        .await
        .expect("other");
    a.link_memories(shared.id, other.id, MemoryRelation::Related)
        .await
        .expect("link");
    a.set_memory_triggers(shared.id, &["database is locked"])
        .await
        .expect("trigger");
    let personal = a
        .save_memory(save(Some(&pa), "Mi atajo personal", None))
        .await
        .expect("personal");
    a.set_memory_scope(personal.id, Visibility::Personal)
        .await
        .expect("scope");
    a.save_memory(save(None, "Preferencia global", None))
        .await
        .expect("global");

    let dir = tmp.path().join("repo").join(SYNC_DIR);
    let counts = a.sync_export(&pa, &dir).await.expect("export");
    assert_eq!((counts.memories, counts.links, counts.triggers), (2, 1, 1));
    let first = std::fs::read(dir.join("memories.jsonl.gz")).expect("file");
    a.sync_export(&pa, &dir).await.expect("export again");
    assert_eq!(
        first,
        std::fs::read(dir.join("memories.jsonl.gz")).expect("file"),
        "deterministic"
    );

    let (b, pb) = db_with_project(tmp.path(), "b.db").await;
    let report = b.sync_import(&pb, &dir).await.expect("import");
    assert_eq!((report.added, report.links, report.triggers), (2, 1, 1));
    let titles: Vec<String> = b
        .recent_memories(saispace_core::models::MemoryScope::All, 50)
        .await
        .expect("list")
        .into_iter()
        .map(|m| m.title)
        .collect();
    assert!(titles.contains(&"Usar WAL".to_string()));
    assert!(
        !titles
            .iter()
            .any(|t| t.contains("personal") || t.contains("global"))
    );

    let again = b.sync_import(&pb, &dir).await.expect("re-import");
    assert_eq!((again.added, again.updated, again.unchanged), (0, 0, 2));
    assert_eq!((again.links, again.triggers), (0, 0));
}

#[tokio::test]
async fn temporary_facts_keep_their_deadline_and_expired_ones_stay_home() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (a, pa) = db_with_project(tmp.path(), "a.db").await;
    let soon = chrono::Utc::now() + chrono::Duration::hours(6);
    let temporary = a
        .save_memory(save(Some(&pa), "Sin cuota", None))
        .await
        .expect("temporary");
    a.set_memory_expiry(temporary.id, Some(soon))
        .await
        .expect("expiry");
    let expired = a
        .save_memory(save(Some(&pa), "Ya caducada", None))
        .await
        .expect("expired");
    a.set_memory_expiry(
        expired.id,
        Some(chrono::Utc::now() - chrono::Duration::hours(1)),
    )
    .await
    .expect("expiry");

    let dir = tmp.path().join("repo").join(SYNC_DIR);
    let counts = a.sync_export(&pa, &dir).await.expect("export");
    assert_eq!(counts.memories, 1);

    let (b, pb) = db_with_project(tmp.path(), "b.db").await;
    b.sync_import(&pb, &dir).await.expect("import");
    let deadline = b.memory_expiry(temporary.id).await.expect("read");
    assert_eq!(deadline.map(|d| d.timestamp()), Some(soon.timestamp()));
}
