use chrono::{Duration, Utc};
use saispace_core::models::{
    CreateProjectInput, MemoryKind, MemoryScope, SaveMemoryInput, SearchKind,
};
use saispace_core::{CoreError, Db};
use std::path::Path;
use uuid::Uuid;

async fn setup() -> Db {
    let db = Db::connect_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    db
}

fn input(title: &str, content: &str, topic: Option<&str>) -> SaveMemoryInput {
    SaveMemoryInput {
        project_id: None,
        project_name: None,
        kind: MemoryKind::Decision,
        title: title.to_string(),
        content: content.to_string(),
        topic_key: topic.map(str::to_string),
    }
}

#[tokio::test]
async fn topic_key_upserts_within_a_project_and_search_follows() {
    let db = setup().await;
    let project = db
        .create_project(CreateProjectInput {
            name: "alpha".into(),
            path: "/p/alpha".into(),
            context_notes: None,
        })
        .await
        .expect("project");

    let first = db
        .save_memory(SaveMemoryInput {
            project_id: Some(project.id),
            ..input(
                "Auth model",
                "Usamos sesiones con cookie",
                Some("arch/auth"),
            )
        })
        .await
        .expect("save");
    assert_eq!(first.project_name.as_deref(), Some("alpha"));

    let updated = db
        .save_memory(SaveMemoryInput {
            project_id: Some(project.id),
            ..input("Auth model", "Migramos a JWT rotativo", Some("arch/auth"))
        })
        .await
        .expect("upsert");
    assert_eq!(updated.id, first.id);
    assert_eq!(updated.content, "Migramos a JWT rotativo");

    let global = db
        .save_memory(input(
            "Regla global",
            "Nunca usar unwrap",
            Some("arch/auth"),
        ))
        .await
        .expect("same topic, other scope");
    assert_ne!(global.id, first.id);
    assert_eq!(global.project_id, None);

    let hits = db
        .search("jwt", Some(project.id), None)
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, SearchKind::Memory);
    assert_eq!(hits[0].id, first.id);
    assert_eq!(db.search("cookie", None, None).await.expect("old").len(), 0);

    let recent = db
        .recent_memories(MemoryScope::Project(project.id), 10)
        .await
        .expect("recent");
    assert_eq!(recent.len(), 2);

    db.delete_memory(global.id).await.expect("delete");
    assert!(matches!(
        db.get_memory(global.id).await,
        Err(CoreError::NotFound {
            entity: "memory",
            ..
        })
    ));
    assert_eq!(
        db.search("unwrap", None, None).await.expect("gone").len(),
        0
    );
}

#[tokio::test]
async fn validation_unknown_project_and_import_timestamps() {
    let db = setup().await;
    assert!(matches!(
        db.save_memory(input("  ", "x", None)).await,
        Err(CoreError::Validation(_))
    ));
    assert!(matches!(
        db.save_memory(SaveMemoryInput {
            project_id: Some(Uuid::now_v7()),
            ..input("t", "c", None)
        })
        .await,
        Err(CoreError::NotFound {
            entity: "project",
            ..
        })
    ));

    let old = Utc::now() - Duration::days(30);
    let imported = db
        .save_memory_at(
            SaveMemoryInput {
                project_name: Some("rome_store".into()),
                ..input("Importada", "desde engram", None)
            },
            old,
            Some(old),
        )
        .await
        .expect("import");
    assert_eq!(imported.created_at.timestamp(), old.timestamp());
    assert_eq!(imported.project_id, None);

    let project = db
        .create_project(CreateProjectInput {
            name: "rome_store".into(),
            path: "/p/rome".into(),
            context_notes: None,
        })
        .await
        .expect("project");
    // create_project links orphan memories saved under its name.
    assert_eq!(
        db.link_memories_by_name(project.id, "rome_store")
            .await
            .expect("link"),
        0
    );
    assert_eq!(
        db.get_memory(imported.id).await.expect("get").project_id,
        Some(project.id)
    );

    let found = db
        .find_project_for_path(Path::new("/p/rome/src/lib.rs"))
        .await
        .expect("lookup");
    assert_eq!(found.map(|p| p.id), Some(project.id));
    assert_eq!(
        db.find_project_for_path(Path::new("/p/romeo"))
            .await
            .expect("no match")
            .map(|p| p.id),
        None
    );
}

#[tokio::test]
async fn open_creates_file_database_with_migrations() {
    let dir = std::env::temp_dir().join(format!("sai db {}", Uuid::now_v7()));
    let path = dir.join("weird #name?.db");
    let db = Db::open(&path).await.expect("open path with spaces and #?");
    db.save_memory(input("t", "c", None)).await.expect("write");
    assert!(path.is_file());
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

fn kind(kind: MemoryKind, title: &str, content: &str) -> SaveMemoryInput {
    SaveMemoryInput {
        kind,
        ..input(title, content, None)
    }
}

#[tokio::test]
async fn unregistered_project_memories_stay_out_of_other_projects() {
    let db = setup().await;
    let mine = db
        .create_project(CreateProjectInput {
            name: "mine".into(),
            path: "/p/mine".into(),
            context_notes: None,
        })
        .await
        .expect("project");
    let orphan = db
        .save_memory(SaveMemoryInput {
            project_name: Some("onix gestoria".into()),
            ..input("Despliegue NAS cliente", "Cloud Sync del cliente", None)
        })
        .await
        .expect("orphan");
    db.save_memory(input("Regla global", "Despliegue siempre con backup", None))
        .await
        .expect("global");

    let visible = db
        .recent_memories(MemoryScope::Project(mine.id), 10)
        .await
        .expect("recent");
    assert_eq!(visible.len(), 1, "only the true global memory");
    assert_eq!(
        db.recent_memories(MemoryScope::GlobalOnly, 10)
            .await
            .expect("global")
            .len(),
        1
    );
    assert_eq!(
        db.recent_memories(MemoryScope::All, 10)
            .await
            .expect("all")
            .len(),
        2
    );
    let hits = db
        .search("despliegue", Some(mine.id), None)
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_ne!(hits[0].id, orphan.id);

    // Opening the client's folder registers it because memories carry its name.
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = dir.path().join("ONIX GESTORIA");
    std::fs::create_dir_all(folder.join("src")).expect("mkdir");
    let found = db
        .project_for_path(&folder.join("src"), false)
        .await
        .expect("resolve")
        .expect("auto project");
    assert_eq!(found.path, folder.to_string_lossy());
    let linked = db.get_memory(orphan.id).await.expect("linked");
    assert_eq!(linked.project_id, Some(found.id));
    assert_eq!(linked.project_name.as_deref(), Some("ONIX GESTORIA"));

    // A folder without memories is only registered when saving.
    let other = dir.path().join("nuevo");
    std::fs::create_dir_all(&other).expect("mkdir");
    assert!(
        db.project_for_path(&other, false)
            .await
            .expect("read")
            .is_none()
    );
    assert!(
        db.project_for_path(&other, true)
            .await
            .expect("save")
            .is_some()
    );
}

#[tokio::test]
async fn context_keeps_preferences_and_only_the_last_session() {
    let db = setup().await;
    let now = Utc::now();
    for (i, item) in [
        kind(MemoryKind::SessionSummary, "Sesión vieja", "Goal: a"),
        kind(
            MemoryKind::Preference,
            "Clippy pedantic",
            "Pasar clippy pedantic",
        ),
        kind(MemoryKind::Decision, "Usar sqlx", "Por migraciones"),
        kind(MemoryKind::SessionSummary, "Sesión nueva", "Goal: b"),
    ]
    .into_iter()
    .enumerate()
    {
        let at = now + Duration::seconds(i64::try_from(i).expect("small"));
        db.save_memory_at(item, at, None).await.expect("save");
    }
    let context = db
        .memory_context(MemoryScope::GlobalOnly, 10)
        .await
        .expect("context");
    assert_eq!(context.preferences.len(), 1);
    assert_eq!(
        context.last_session.map(|m| m.title).as_deref(),
        Some("Sesión nueva")
    );
    assert_eq!(context.memories.len(), 1);
}

#[tokio::test]
async fn search_falls_back_to_any_term_and_finds_similar_memories() {
    let db = setup().await;
    let vault = db
        .save_memory(input("Baúl de claves", "ChaCha20 con clave local", None))
        .await
        .expect("save");
    let hits = db
        .search("cifrado claves", None, None)
        .await
        .expect("search");
    assert_eq!(hits.first().map(|h| h.id), Some(vault.id));

    let twin = db
        .save_memory(input("Claves del baúl cifradas", "duplicado", None))
        .await
        .expect("twin");
    let similar = db.similar_memories(&twin, 3).await.expect("similar");
    assert_eq!(
        similar.iter().map(|h| h.id).collect::<Vec<_>>(),
        vec![vault.id]
    );
}
