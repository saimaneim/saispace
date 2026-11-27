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
        })
        .await
        .expect("project a");

    let proj_b = db
        .create_project(CreateProjectInput {
            name: "proj-b".into(),
            path: "/p/proj-b".into(),
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

#[allow(clippy::too_many_lines)] // one end-to-end scenario reads best in one piece
async fn seed_mixed_memories(total: usize) -> (Db, uuid::Uuid, uuid::Uuid) {
    use chrono::{Duration, Utc};
    let db = setup().await;
    let proj_a = db
        .create_project(CreateProjectInput {
            name: "proj-a".into(),
            path: "/p/proj-a".into(),
        })
        .await
        .expect("proj_a");
    let proj_b = db
        .create_project(CreateProjectInput {
            name: "proj-b".into(),
            path: "/p/proj-b".into(),
        })
        .await
        .expect("proj_b");

    let base_time = Utc::now() - Duration::days(5);
    let mut seq = 0_i64;
    let mut next_ts = || {
        seq += 1;
        base_time + Duration::seconds(seq)
    };

    // Real duplicate pair in proj_a, placed early so it sits near the 200th position.
    db.save_memory_at(
        SaveMemoryInput {
            project_id: Some(proj_a.id),
            project_name: None,
            kind: MemoryKind::Config,
            title: "Nginx reverse proxy upload limits configuration".into(),
            content: "What: set client_max_body_size 10m".into(),
            topic_key: None,
        },
        next_ts(),
        None,
    )
    .await
    .expect("pa dup 1");
    db.save_memory_at(
        SaveMemoryInput {
            project_id: Some(proj_a.id),
            project_name: None,
            kind: MemoryKind::Config,
            title: "Nginx reverse proxy upload limit settings".into(),
            content: "What: client_max_body_size 10m".into(),
            topic_key: None,
        },
        next_ts(),
        None,
    )
    .await
    .expect("pa dup 2");

    // 15 cross-project distractors in proj_b sharing the same title words.
    for i in 0..15 {
        db.save_memory_at(
            SaveMemoryInput {
                project_id: Some(proj_b.id),
                project_name: None,
                kind: MemoryKind::Config,
                title: format!("Nginx reverse proxy upload limits variant {i:02}"),
                content: format!("Distractor {i}"),
                topic_key: None,
            },
            next_ts(),
            None,
        )
        .await
        .expect("distractor");
    }

    // Real duplicate pair in global scope.
    db.save_memory_at(
        SaveMemoryInput {
            project_id: None,
            project_name: None,
            kind: MemoryKind::Preference,
            title: "Always use biome formatter in frontend".into(),
            content: "Use biome".into(),
            topic_key: None,
        },
        next_ts(),
        None,
    )
    .await
    .expect("global dup 1");
    db.save_memory_at(
        SaveMemoryInput {
            project_id: None,
            project_name: None,
            kind: MemoryKind::Preference,
            title: "Always use biome linter in frontend".into(),
            content: "Use biome".into(),
            topic_key: None,
        },
        next_ts(),
        None,
    )
    .await
    .expect("global dup 2");

    // Same-title memories across proj_a, proj_b and global must never pair across boundaries.
    db.save_memory_at(
        SaveMemoryInput {
            project_id: Some(proj_a.id),
            project_name: None,
            kind: MemoryKind::Decision,
            title: "Unique cross scope protocol design".into(),
            content: "A".into(),
            topic_key: None,
        },
        next_ts(),
        None,
    )
    .await
    .expect("cross a");
    db.save_memory_at(
        SaveMemoryInput {
            project_id: Some(proj_b.id),
            project_name: None,
            kind: MemoryKind::Decision,
            title: "Unique cross scope protocol design".into(),
            content: "B".into(),
            topic_key: None,
        },
        next_ts(),
        None,
    )
    .await
    .expect("cross b");
    db.save_memory_at(
        SaveMemoryInput {
            project_id: None,
            project_name: None,
            kind: MemoryKind::Decision,
            title: "Unique cross scope protocol design".into(),
            content: "G".into(),
            topic_key: None,
        },
        next_ts(),
        None,
    )
    .await
    .expect("cross g");

    // Fill the remainder with unique, non-duplicate memories across proj_a, proj_b and global.
    let seeded_so_far = 2 + 15 + 2 + 3;
    for idx in seeded_so_far..total {
        let (project_id, kind) = match idx % 3 {
            0 => (Some(proj_a.id), MemoryKind::Discovery),
            1 => (Some(proj_b.id), MemoryKind::Pattern),
            _ => (None, MemoryKind::Note),
        };
        db.save_memory_at(
            SaveMemoryInput {
                project_id,
                project_name: None,
                kind,
                title: format!("Standalone entry alpha{idx} beta{idx} gamma{idx}"),
                content: format!("Body {idx}"),
                topic_key: None,
            },
            next_ts(),
            None,
        )
        .await
        .expect("filler");
    }

    (db, proj_a.id, proj_b.id)
}

fn pair_signatures(pairs: &[saispace_core::DuplicatePair]) -> Vec<(String, String)> {
    let mut sigs: Vec<_> = pairs
        .iter()
        .map(|p| {
            if p.a.title <= p.b.title {
                (p.a.title.clone(), p.b.title.clone())
            } else {
                (p.b.title.clone(), p.a.title.clone())
            }
        })
        .collect();
    sigs.sort();
    sigs
}

#[tokio::test]
async fn two_hundred_and_two_hundred_one_memories_return_same_pairs_and_context_matches() {
    let (db_200, pa_200, _) = seed_mixed_memories(200).await;
    let (db_201, pa_201, _) = seed_mixed_memories(201).await;

    for scope_pair in [
        (MemoryScope::All, MemoryScope::All),
        (MemoryScope::GlobalOnly, MemoryScope::GlobalOnly),
        (MemoryScope::Project(pa_200), MemoryScope::Project(pa_201)),
    ] {
        let pairs_200 = db_200
            .duplicate_pairs(scope_pair.0, 50)
            .await
            .expect("dupes 200");
        let pairs_201 = db_201
            .duplicate_pairs(scope_pair.1, 50)
            .await
            .expect("dupes 201");
        assert_eq!(pair_signatures(&pairs_200), pair_signatures(&pairs_201));
    }

    // Check Project(pa) specifically finds both proj_a and global duplicates, never cross-project.
    let pa_pairs = db_201
        .duplicate_pairs(MemoryScope::Project(pa_201), 50)
        .await
        .expect("pa pairs");
    let sigs = pair_signatures(&pa_pairs);
    assert!(sigs.iter().any(|(a, _)| a.contains("Nginx reverse proxy")));
    assert!(sigs.iter().any(|(a, _)| a.contains("Always use biome")));
    assert!(!sigs.iter().any(|(a, _)| a.contains("Unique cross scope")));

    // Check memory_context_with_dupes matches memory_context across scopes at 200 and 201.
    for (db, scope) in [
        (&db_200, MemoryScope::All),
        (&db_200, MemoryScope::GlobalOnly),
        (&db_200, MemoryScope::Project(pa_200)),
        (&db_201, MemoryScope::All),
        (&db_201, MemoryScope::GlobalOnly),
        (&db_201, MemoryScope::Project(pa_201)),
    ] {
        let ctx_auto = db.memory_context(scope, 12).await.expect("memory_context");
        let dupes = db.duplicate_pairs(scope, 50).await.expect("dupes");
        let ctx_with = db
            .memory_context_with_dupes(scope, 12, &dupes)
            .await
            .expect("memory_context_with_dupes");
        assert_eq!(ctx_auto.consolidate, ctx_with.consolidate);
        assert_eq!(ctx_auto.memories.len(), ctx_with.memories.len());
        assert_eq!(ctx_auto.preferences.len(), ctx_with.preferences.len());
        assert_eq!(ctx_auto.pinned.len(), ctx_with.pinned.len());
    }
}
