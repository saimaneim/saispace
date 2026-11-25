use crate::db::Db;
use crate::db::memory_edit::is_unique_constraint_violation;
use crate::error::CoreResult;
use crate::topic_key::normalize_topic_key;
use std::collections::HashMap;
use uuid::Uuid;

struct TopicRow {
    id: Uuid,
    project_name: String,
    current: String,
    target: String,
}

impl Db {
    /// Rewrites stored `topic_key`s with the current convention
    /// ([`normalize_topic_key`]): separators unified, accents and project
    /// prefixes stripped, legacy `engram/<id>` keys cleared.
    ///
    /// A row whose normalized key is already claimed by another row is
    /// skipped instead of violating `idx_memories_topic`, so the caller
    /// can merge those by hand. Returns how many rows changed. Data
    /// migration for `sai memory normalize-topics`; never run on startup.
    pub async fn normalize_existing_topic_keys(&self) -> CoreResult<usize> {
        let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT id, project_name, topic_key FROM memories WHERE topic_key <> ''",
        )
        .fetch_all(&self.pool)
        .await?;

        let entries: Vec<TopicRow> = rows
            .into_iter()
            .map(|(id, project_name, current)| {
                let target = normalize_topic_key(&current, &project_name);
                TopicRow {
                    id,
                    project_name,
                    current,
                    target,
                }
            })
            .collect();

        // Count every (project, target) slot first: a row that already holds
        // the normalized key occupies it too, so two rows fighting for one
        // slot (or one fighting an incumbent) both stay untouched.
        let mut claims: HashMap<(&str, &str), usize> = HashMap::new();
        for entry in &entries {
            if !entry.target.is_empty() {
                *claims
                    .entry((entry.project_name.as_str(), entry.target.as_str()))
                    .or_insert(0) += 1;
            }
        }

        let planned: Vec<(Uuid, &str)> = entries
            .iter()
            .filter(|entry| {
                if entry.target == entry.current {
                    return false;
                }
                entry.target.is_empty()
                    || claims
                        .get(&(entry.project_name.as_str(), entry.target.as_str()))
                        .is_some_and(|n| *n == 1)
            })
            .map(|entry| (entry.id, entry.target.as_str()))
            .collect();

        let mut changed = 0;
        for (id, target) in planned {
            let result = sqlx::query("UPDATE memories SET topic_key = ?1 WHERE id = ?2")
                .bind(target)
                .bind(id)
                .execute(&self.pool)
                .await;
            match result {
                Ok(res) if res.rows_affected() == 1 => changed += 1,
                // The row vanished between the SELECT and the UPDATE.
                Ok(_) => {}
                // A concurrent writer claimed the slot first: skip.
                Err(err) if is_unique_constraint_violation(&err) => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::Db;
    use crate::models::{MemoryKind, SaveMemoryInput};
    use uuid::Uuid;

    /// Saves a memory with a raw, unnormalized `topic_key`, bypassing the
    /// normalization now applied on save.
    async fn raw_topic(db: &Db, project_name: &str, title: &str, topic: &str) -> Uuid {
        let saved = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: Some(project_name.to_string()),
                kind: MemoryKind::Decision,
                title: title.to_string(),
                content: "x".to_string(),
                // Distinct per call so the upsert does not overwrite the
                // previous row before we overwrite the key by hand.
                topic_key: Some(format!("seed-{}", Uuid::now_v7().simple())),
            })
            .await
            .expect("save");
        sqlx::query("UPDATE memories SET topic_key = ?1 WHERE id = ?2")
            .bind(topic)
            .bind(saved.id)
            .execute(db.pool())
            .await
            .expect("raw topic");
        saved.id
    }

    async fn topic_of(db: &Db, id: Uuid) -> String {
        let memory = db.get_memory(id).await.expect("get");
        memory.topic_key.unwrap_or_default()
    }

    #[tokio::test]
    async fn normalizes_prefixes_and_clears_legacy_keys() {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");

        let prefixed = raw_topic(&db, "saispace", "UI rules", "saispace/UI_Design.Rules").await;
        let legacy = raw_topic(&db, "saispace", "Old note", "engram/14").await;
        let clean = raw_topic(&db, "saispace", "Already clean", "already-clean").await;

        let changed = db.normalize_existing_topic_keys().await.expect("normalize");
        assert_eq!(changed, 2);
        assert_eq!(topic_of(&db, prefixed).await, "ui-design-rules");
        assert_eq!(topic_of(&db, legacy).await, "");
        assert_eq!(topic_of(&db, clean).await, "already-clean");

        // Idempotent: a second run has nothing left to do.
        assert_eq!(db.normalize_existing_topic_keys().await.expect("again"), 0);
    }

    #[tokio::test]
    async fn skips_rows_that_would_collide() {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");

        let messy = raw_topic(&db, "onix", "Auth messy", "Auth_Model").await;
        let incumbent = raw_topic(&db, "onix", "Auth clean", "auth-model").await;
        let other_project = raw_topic(&db, "crm", "Auth in CRM", "Auth_Model").await;

        let changed = db.normalize_existing_topic_keys().await.expect("normalize");
        assert_eq!(changed, 1);

        // Both "onix" rows claim "auth-model": neither moves.
        assert_eq!(topic_of(&db, messy).await, "Auth_Model");
        assert_eq!(topic_of(&db, incumbent).await, "auth-model");
        // The same raw key in another project does not collide.
        assert_eq!(topic_of(&db, other_project).await, "auth-model");
    }

    #[tokio::test]
    async fn keeps_scopes_separate_and_updates_still_work() {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");

        let global = raw_topic(&db, "", "Global rule", "Global/Rule.Set").await;
        let changed = db.normalize_existing_topic_keys().await.expect("normalize");
        assert_eq!(changed, 1);
        assert_eq!(topic_of(&db, global).await, "global-rule-set");

        // After migration the upsert path still matches the new key.
        let again = db
            .save_memory(SaveMemoryInput {
                project_id: None,
                project_name: Some(String::new()),
                kind: MemoryKind::Note,
                title: "Global rule v2".to_string(),
                content: "updated".to_string(),
                topic_key: Some("global-rule-set".to_string()),
            })
            .await
            .expect("upsert");
        assert_eq!(again.id, global);
        assert_eq!(again.content, "updated");
        assert!(again.updated_at > again.created_at);
    }
}
