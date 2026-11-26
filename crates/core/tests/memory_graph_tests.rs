use saispace_core::models::{
    ConsolidateInput, CreateProjectInput, LinkDirection, MemoryKind, MemoryRelation, MemoryScope,
    SaveMemoryInput,
};
use saispace_core::{Db, KEEP_SESSIONS, short_id};
use uuid::Uuid;

async fn setup() -> (Db, Uuid) {
    let db = Db::connect_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    let project = db
        .create_project(CreateProjectInput {
            name: "alpha".into(),
            path: "/p/alpha".into(),
        })
        .await
        .expect("project");
    (db, project.id)
}

fn input(project: Uuid, kind: MemoryKind, title: &str, content: &str) -> SaveMemoryInput {
    SaveMemoryInput {
        project_id: Some(project),
        project_name: None,
        kind,
        title: title.to_string(),
        content: content.to_string(),
        topic_key: None,
    }
}

#[tokio::test]
async fn short_ids_resolve_and_reject_ambiguity_and_garbage() {
    let (db, project) = setup().await;
    let saved = db
        .save_memory(input(project, MemoryKind::Note, "Una nota", "texto"))
        .await
        .expect("save");
    let short = short_id(saved.id);
    assert_eq!(short.len(), 8);
    assert_eq!(db.resolve_memory_id(&short).await.expect("short"), saved.id);
    assert_eq!(
        db.resolve_memory_id(&saved.id.to_string())
            .await
            .expect("full"),
        saved.id
    );
    assert!(db.resolve_memory_id("zzzz").await.is_err());
    assert!(db.resolve_memory_id("0000000000").await.is_err());
}

#[tokio::test]
async fn saving_links_memories_mentioned_by_topic_or_id() {
    let (db, project) = setup().await;
    let api = db
        .save_memory(SaveMemoryInput {
            topic_key: Some("opencode-plugin-api-v2".into()),
            ..input(
                project,
                MemoryKind::Discovery,
                "API v2 de plugins",
                "session.instructions",
            )
        })
        .await
        .expect("api");
    let other = db
        .save_memory(input(project, MemoryKind::Note, "Otra", "sin relación"))
        .await
        .expect("other");
    let summary = db
        .save_memory(input(
            project,
            MemoryKind::SessionSummary,
            "Sesión",
            &format!(
                "Plugin portado (ver opencode-plugin-api-v2). Ver también {}.",
                short_id(other.id)
            ),
        ))
        .await
        .expect("summary");

    let links = db.memory_links(summary.id).await.expect("links");
    let ids: Vec<Uuid> = links.iter().map(|l| l.id).collect();
    assert!(ids.contains(&api.id));
    assert!(ids.contains(&other.id));
    assert!(links.iter().all(|l| l.relation == MemoryRelation::Related));
    assert!(links.iter().all(|l| l.direction == LinkDirection::Out));

    let back = db.memory_links(api.id).await.expect("backlinks");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].direction, LinkDirection::In);
}

#[tokio::test]
async fn manual_links_are_idempotent_and_never_self() {
    let (db, project) = setup().await;
    let a = db
        .save_memory(input(project, MemoryKind::Decision, "A", "a"))
        .await
        .expect("a");
    let b = db
        .save_memory(input(project, MemoryKind::Bugfix, "B", "b"))
        .await
        .expect("b");
    assert!(
        db.link_memories(b.id, a.id, MemoryRelation::DependsOn)
            .await
            .expect("link")
    );
    assert!(
        !db.link_memories(b.id, a.id, MemoryRelation::DependsOn)
            .await
            .expect("again")
    );
    assert!(
        db.link_memories(a.id, a.id, MemoryRelation::Related)
            .await
            .is_err()
    );
    assert_eq!(db.unlink_memories(a.id, b.id).await.expect("unlink"), 1);
    assert_eq!(db.memory_links(a.id).await.expect("links"), Vec::new());
}

#[tokio::test]
async fn consolidation_merges_into_one_and_archives_the_rest() {
    let (db, project) = setup().await;
    let a = db
        .save_memory(SaveMemoryInput {
            topic_key: Some("auth-model".into()),
            ..input(
                project,
                MemoryKind::Decision,
                "Auth con cookies",
                "Sesiones con cookie httpOnly",
            )
        })
        .await
        .expect("a");
    let b = db
        .save_memory(input(
            project,
            MemoryKind::Decision,
            "Auth cookies v2",
            "Cookie SameSite=Lax",
        ))
        .await
        .expect("b");
    let c = db
        .save_memory(input(
            project,
            MemoryKind::Note,
            "Nota suelta",
            "depende de auth",
        ))
        .await
        .expect("c");
    db.link_memories(c.id, b.id, MemoryRelation::DependsOn)
        .await
        .expect("c->b");
    db.touch_memories(&[a.id, b.id, b.id]).await.expect("touch");

    let merged = db
        .consolidate_memories(ConsolidateInput {
            ids: vec![b.id, a.id],
            title: "Auth: sesiones con cookie".into(),
            content: "Cookie httpOnly, SameSite=Lax".into(),
            kind: None,
            topic_key: None,
        })
        .await
        .expect("consolidate");

    // The memory with a topic_key survives and keeps it.
    assert_eq!(merged.id, a.id);
    assert_eq!(merged.topic_key.as_deref(), Some("auth-model"));
    assert_eq!(merged.content, "Cookie httpOnly, SameSite=Lax");
    assert_eq!(merged.use_count, 2);

    let archived = db.get_memory(b.id).await.expect("b still readable");
    assert_eq!(archived.superseded_by, Some(a.id));

    // Out of the index and of search, still reachable through the link.
    let context = db
        .memory_context(MemoryScope::Project(project), 10)
        .await
        .expect("context");
    assert!(context.memories.iter().all(|m| m.id != b.id));
    let hits = db
        .search("SameSite", Some(project), None)
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, a.id);

    let links = db.memory_links(a.id).await.expect("links");
    assert!(
        links
            .iter()
            .any(|l| l.id == b.id && l.relation == MemoryRelation::Supersedes && l.archived)
    );
    // c depended on b; now it depends on the survivor.
    assert!(links.iter().any(|l| l.id == c.id
        && l.relation == MemoryRelation::DependsOn
        && l.direction == LinkDirection::In));

    // Saving again with the topic updates the survivor, not the archived row.
    let again = db
        .save_memory(SaveMemoryInput {
            topic_key: Some("auth-model".into()),
            ..input(project, MemoryKind::Decision, "Auth", "actualizado")
        })
        .await
        .expect("upsert");
    assert_eq!(again.id, a.id);

    let twice = db
        .consolidate_memories(ConsolidateInput {
            ids: vec![a.id, b.id],
            title: "x".into(),
            content: "y".into(),
            kind: None,
            topic_key: None,
        })
        .await;
    assert!(twice.is_err(), "an archived memory cannot be merged again");
}

#[tokio::test]
async fn consolidation_refuses_mixed_projects() {
    let (db, project) = setup().await;
    let local = db
        .save_memory(input(project, MemoryKind::Note, "Local", "x"))
        .await
        .expect("local");
    let global = db
        .save_memory(SaveMemoryInput {
            project_id: None,
            ..input(project, MemoryKind::Note, "Global", "y")
        })
        .await
        .expect("global");
    let result = db
        .consolidate_memories(ConsolidateInput {
            ids: vec![local.id, global.id],
            title: "t".into(),
            content: "c".into(),
            kind: None,
            topic_key: None,
        })
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn only_the_latest_session_summaries_stay_active() {
    let (db, project) = setup().await;
    let mut ids = Vec::new();
    for n in 0..(KEEP_SESSIONS + 2) {
        let saved = db
            .save_memory(input(
                project,
                MemoryKind::SessionSummary,
                &format!("Sesión {n}"),
                "Goal: algo",
            ))
            .await
            .expect("summary");
        ids.push(saved.id);
    }
    let newest = *ids.last().expect("newest");
    for (i, id) in ids.iter().enumerate() {
        let memory = db.get_memory(*id).await.expect("get");
        let should_archive = i < ids.len() - KEEP_SESSIONS;
        assert_eq!(
            memory.superseded_by.is_some(),
            should_archive,
            "summary {i}"
        );
        if should_archive {
            assert!(memory.superseded_by.is_some_and(|by| by != *id));
        }
    }
    let context = db
        .memory_context(MemoryScope::Project(project), 10)
        .await
        .expect("context");
    assert_eq!(context.last_session.map(|m| m.id), Some(newest));
}
