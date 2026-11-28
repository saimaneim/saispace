//! `sai memory tidy`: one deterministic maintenance pass over the checks that
//! already exist (look-alike pairs, unused memories, surplus session
//! summaries, orphans). It only plans by default; `apply` merges pairs,
//! archives surplus summaries and attaches orphans. Nothing is deleted.

use crate::db::Db;
use crate::db::memory_dupes::DuplicatePair;
use crate::db::memory_orphans::OrphanGroup;
use crate::error::CoreResult;
use crate::models::{ConsolidateInput, Memory, MemoryScope, Project};
use crate::paths::project_key;
use serde::Serialize;

/// Pairs considered per pass: enough to make progress, small enough to review.
const MAX_PAIRS: usize = 20;

#[derive(Debug, Clone, Default, Serialize)]
pub struct TidyPlan {
    pub duplicates: Vec<DuplicatePair>,
    /// Unused for a long time: listed for review only, never changed.
    pub stale: Vec<Memory>,
    pub surplus_sessions: Vec<Memory>,
    /// Orphan groups whose name matches a registered project.
    pub orphans: Vec<(OrphanGroup, Project)>,
}

impl TidyPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.duplicates.is_empty()
            && self.stale.is_empty()
            && self.surplus_sessions.is_empty()
            && self.orphans.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TidyResult {
    pub merged: usize,
    pub archived_sessions: u64,
    pub attached_orphans: u64,
}

/// Content of a merge: the newer text, plus the older one when it says
/// something the newer does not already contain.
fn merged_content(newer: &Memory, older: &Memory) -> String {
    let (new, old) = (newer.content.trim(), older.content.trim());
    if new.contains(old) {
        new.to_string()
    } else {
        format!("{new}\n\n{old}")
    }
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if a check cannot run.
    pub async fn tidy_plan(
        &self,
        scope: MemoryScope,
        days: i64,
        keep: i64,
    ) -> CoreResult<TidyPlan> {
        let report = self.stale_report(scope, days, keep).await?;
        let projects = self.list_projects().await?;
        let orphans = self
            .orphan_memory_groups()
            .await?
            .into_iter()
            .filter_map(|g| {
                let key = project_key(&g.project_name);
                let project = projects.iter().find(|p| project_key(&p.name) == key)?;
                let in_scope = match scope {
                    MemoryScope::All => true,
                    MemoryScope::GlobalOnly => false,
                    MemoryScope::Project(id) => project.id == id,
                };
                in_scope.then(|| (g, project.clone()))
            })
            .collect();
        // A personal note is never merged into a shared one (or the reverse):
        // the merged text would change who can see it.
        let duplicates = self
            .duplicate_pairs(scope, MAX_PAIRS)
            .await?
            .into_iter()
            .filter(|pair| pair.a.scope == pair.b.scope)
            .collect();
        Ok(TidyPlan {
            duplicates,
            stale: report.stale,
            surplus_sessions: report.surplus_sessions,
            orphans,
        })
    }

    /// Applies `plan`. Each merge keeps the newer title and both texts; the
    /// older memory is archived pointing to the survivor.
    ///
    /// # Errors
    ///
    /// Returns an error if a change fails; earlier changes stay applied.
    pub async fn apply_tidy(&self, plan: &TidyPlan) -> CoreResult<TidyResult> {
        let mut result = TidyResult::default();
        let mut touched = std::collections::HashSet::new();
        for pair in &plan.duplicates {
            // A memory merged earlier in this pass is already archived.
            if !touched.insert(pair.a.id) | !touched.insert(pair.b.id) {
                continue;
            }
            let (newer, older) = if pair.a.updated_at >= pair.b.updated_at {
                (&pair.a, &pair.b)
            } else {
                (&pair.b, &pair.a)
            };
            self.consolidate_memories(ConsolidateInput {
                ids: vec![newer.id, older.id],
                title: newer.title.clone(),
                content: merged_content(newer, older),
                kind: None,
                topic_key: None,
            })
            .await?;
            result.merged += 1;
        }
        for m in &plan.surplus_sessions {
            result.archived_sessions += sqlx::query(
                r"
                UPDATE memories SET superseded_by = (
                    SELECT id FROM memories n
                    WHERE n.kind = 'session_summary' AND n.superseded_by IS NULL AND n.id <> ?1
                      AND ((?2 IS NOT NULL AND n.project_id = ?2)
                           OR (?2 IS NULL AND n.project_id IS NULL AND n.project_name = ?3))
                    ORDER BY n.updated_at DESC, n.id DESC LIMIT 1)
                WHERE id = ?1 AND superseded_by IS NULL
                ",
            )
            .bind(m.id)
            .bind(m.project_id)
            .bind(m.project_name.clone().unwrap_or_default())
            .execute(self.pool())
            .await?
            .rows_affected();
        }
        for (group, project) in &plan.orphans {
            result.attached_orphans += self
                .link_orphans_to_project(&group.project_name, project.id)
                .await?;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use crate::models::{MemoryKind, MemoryScope, SaveMemoryInput};
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn plan_changes_nothing_and_apply_merges_and_archives() {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");
        let at = |title: &str, content: &str, kind, ago: i64| {
            let db = db.clone();
            let input = SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind,
                title: title.into(),
                content: content.into(),
                topic_key: None,
            };
            async move {
                db.save_memory_at(input, Utc::now() - Duration::minutes(ago), None)
                    .await
                    .expect("save")
            }
        };
        at(
            "Usar SQLite en modo WAL",
            "Lectores concurrentes.",
            MemoryKind::Decision,
            10,
        )
        .await;
        at(
            "Usar SQLite en modo WAL",
            "Y busy_timeout de 5 s.",
            MemoryKind::Decision,
            5,
        )
        .await;
        for i in 0..5 {
            at(
                &format!("Sesión {i}"),
                "Resumen",
                MemoryKind::SessionSummary,
                100 - i,
            )
            .await;
        }

        let plan = db.tidy_plan(MemoryScope::All, 60, 3).await.expect("plan");
        assert_eq!(plan.duplicates.len(), 1);
        assert_eq!(plan.surplus_sessions.len(), 2);
        let again = db.tidy_plan(MemoryScope::All, 60, 3).await.expect("plan");
        assert_eq!(again.surplus_sessions.len(), 2, "planning is read-only");

        let done = db.apply_tidy(&plan).await.expect("apply");
        assert_eq!((done.merged, done.archived_sessions), (1, 2));
        let after = db.tidy_plan(MemoryScope::All, 60, 3).await.expect("plan");
        assert!(after.duplicates.is_empty() && after.surplus_sessions.is_empty());
        let survivor = db
            .recent_memories(MemoryScope::All, 50)
            .await
            .expect("list")
            .into_iter()
            .find(|m| m.title.contains("WAL"))
            .expect("survivor");
        assert!(survivor.content.contains("busy_timeout") && survivor.content.contains("Lectores"));
    }
}
