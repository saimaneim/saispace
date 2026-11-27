use saispace_core::Db;
use saispace_core::models::{ConsolidateInput, MemoryKind, MemoryScope, SaveMemoryInput};
use saispace_core::recall::{MAX_TRIGGERS, normalize_text};

async fn setup() -> Db {
    let db = Db::connect_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    db
}

async fn global(db: &Db, title: &str) -> uuid::Uuid {
    db.save_memory(SaveMemoryInput {
        project_id: None,
        project_name: None,
        kind: MemoryKind::Bugfix,
        title: title.to_string(),
        content: "Learned: x".to_string(),
        topic_key: None,
    })
    .await
    .expect("save")
    .id
}

#[tokio::test]
async fn triggers_fire_cascade_on_delete_and_reject_bad_input() {
    let db = setup().await;
    let id = global(&db, "ssh sin agente").await;
    let set = db
        .set_memory_triggers(id, &["Permission denied (publickey)", "env:t3code"])
        .await
        .expect("set");
    assert_eq!(set, vec!["permission denied (publickey)", "env:t3code"]);

    let hay = normalize_text("root@h: Permission denied (publickey).");
    let hits = db
        .triggered_memories(MemoryScope::GlobalOnly, &hay, &[], false)
        .await
        .expect("match");
    assert_eq!(hits.len(), 1);
    let env = db
        .triggered_memories(MemoryScope::GlobalOnly, "", &["t3code"], true)
        .await
        .expect("env");
    assert_eq!(env.len(), 1);

    assert!(db.set_memory_triggers(id, &["error"]).await.is_err());
    assert_eq!(db.memory_triggers(id).await.expect("kept").len(), 2);

    db.delete_memory(id).await.expect("delete");
    let after = db
        .triggered_memories(MemoryScope::GlobalOnly, &hay, &[], false)
        .await
        .expect("after delete");
    assert_eq!(after.len(), 0);
}

#[tokio::test]
async fn consolidation_moves_triggers_within_the_limit() {
    let db = setup().await;
    let a = global(&db, "pkill a").await;
    let b = global(&db, "pkill b").await;
    let first: Vec<String> = (0..6).map(|i| format!("signature a{i}")).collect();
    let second: Vec<String> = (0..6).map(|i| format!("signature b{i}")).collect();
    db.set_memory_triggers(a, &first).await.expect("a");
    db.set_memory_triggers(b, &second).await.expect("b");

    let merged = db
        .consolidate_memories(ConsolidateInput {
            ids: vec![a, b],
            title: "pkill".to_string(),
            content: "Learned: y".to_string(),
            kind: None,
            topic_key: None,
        })
        .await
        .expect("consolidate");
    let absorbed = if merged.id == a { b } else { a };
    let kept = db.memory_triggers(merged.id).await.expect("survivor");
    assert_eq!(kept.len(), MAX_TRIGGERS);
    assert_eq!(
        db.memory_triggers(absorbed).await.expect("absorbed"),
        Vec::<String>::new()
    );
    let hay = normalize_text(&kept[0]);
    let hits = db
        .triggered_memories(MemoryScope::GlobalOnly, &hay, &[], false)
        .await
        .expect("match");
    assert_eq!(
        hits.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![merged.id]
    );
    db.set_memory_triggers(merged.id, &kept)
        .await
        .expect("still valid");
}
