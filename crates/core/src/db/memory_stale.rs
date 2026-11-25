use crate::db::Db;
use crate::db::memory::MEMORY_COLUMNS;
use crate::error::CoreResult;
use crate::models::{Memory, MemoryScope};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Review list for memories that may no longer earn their place: the ones
/// nobody has touched in a while, plus the session summaries that piled up
/// past the per-project keep count. Nothing here is deleted automatically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleReport {
    pub stale: Vec<Memory>,
    pub surplus_sessions: Vec<Memory>,
}

/// Cap so an absurd `--days` value cannot overflow the cutoff arithmetic.
const MAX_STALE_DAYS: i64 = 36_500;

fn scope_parts(scope: MemoryScope) -> (i64, Option<Uuid>) {
    match scope {
        MemoryScope::All => (0, None),
        MemoryScope::GlobalOnly => (1, None),
        MemoryScope::Project(id) => (2, Some(id)),
    }
}

/// Group key for session summaries: one project, one orphan folder name,
/// or the global bucket. Memories of the same project always share its id.
fn session_group(m: &Memory) -> (Option<Uuid>, String) {
    // A registered project groups by id alone: renaming it does not rewrite
    // `memories.project_name`, so the name would split one project in two.
    match m.project_id {
        Some(id) => (Some(id), String::new()),
        None => (None, m.project_name.clone().unwrap_or_default()),
    }
}

impl Db {
    /// Memories whose last activity (update or use) is older than `days`
    /// days, oldest first. Preferences, pinned memories and session
    /// summaries are never listed here: summaries have their own keep rule.
    pub async fn stale_memories(&self, scope: MemoryScope, days: i64) -> CoreResult<Vec<Memory>> {
        let cutoff = Utc::now() - Duration::days(days.clamp(0, MAX_STALE_DAYS));
        let (mode, project_id) = scope_parts(scope);
        // Timestamps are stored as sqlx-formatted UTC text, so plain string
        // comparison orders them chronologically.
        let sql = format!(
            r"
            SELECT {MEMORY_COLUMNS}
            FROM memories
            WHERE (?1 = 0
                   OR (project_id IS NULL AND project_name = '')
                   OR (?1 = 2 AND project_id = ?2))
              AND kind NOT IN ('preference', 'session_summary')
              AND pinned = 0
              AND MAX(updated_at, COALESCE(last_used_at, '')) < ?3
            ORDER BY MAX(updated_at, COALESCE(last_used_at, '')) ASC, id ASC
            "
        );
        let items = sqlx::query_as::<_, Memory>(&sql)
            .bind(mode)
            .bind(project_id)
            .bind(cutoff)
            .fetch_all(&self.pool)
            .await?;
        Ok(items)
    }

    /// Session summaries beyond the newest `keep` of each project (global
    /// ones form their own group), oldest first. Pinned summaries are kept. These are the only
    /// candidates `sai memory stale --prune-sessions` may delete.
    pub async fn surplus_session_summaries(
        &self,
        scope: MemoryScope,
        keep: i64,
    ) -> CoreResult<Vec<Memory>> {
        let keep = keep.max(0);
        let (mode, project_id) = scope_parts(scope);
        let sql = format!(
            r"
            SELECT {MEMORY_COLUMNS}
            FROM memories
            WHERE kind = 'session_summary'
              AND pinned = 0
              AND (?1 = 0
                   OR (project_id IS NULL AND project_name = '')
                   OR (?1 = 2 AND project_id = ?2))
            ORDER BY updated_at DESC, created_at DESC, id DESC
            "
        );
        let summaries = sqlx::query_as::<_, Memory>(&sql)
            .bind(mode)
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

        let mut kept: HashMap<(Option<Uuid>, String), i64> = HashMap::new();
        let mut surplus = Vec::new();
        for m in summaries {
            let count = kept.entry(session_group(&m)).or_insert(0);
            if *count < keep {
                *count += 1;
            } else {
                surplus.push(m);
            }
        }
        surplus.reverse();
        Ok(surplus)
    }

    pub async fn stale_report(
        &self,
        scope: MemoryScope,
        days: i64,
        keep: i64,
    ) -> CoreResult<StaleReport> {
        let stale = self.stale_memories(scope, days).await?;
        let surplus_sessions = self.surplus_session_summaries(scope, keep).await?;
        Ok(StaleReport {
            stale,
            surplus_sessions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateProjectInput, MemoryKind, SaveMemoryInput};
    use chrono::DateTime;

    async fn setup() -> Db {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");
        db
    }

    fn days_ago(days: i64) -> DateTime<Utc> {
        Utc::now() - Duration::days(days)
    }

    async fn save_at(
        db: &Db,
        kind: MemoryKind,
        title: &str,
        project_id: Option<Uuid>,
        project_name: Option<&str>,
        age_days: i64,
    ) -> Memory {
        let ts = days_ago(age_days);
        db.save_memory_at(
            SaveMemoryInput {
                project_id,
                project_name: project_name.map(str::to_string),
                kind,
                title: title.into(),
                content: format!("Contenido de {title}"),
                topic_key: None,
            },
            ts,
            Some(ts),
        )
        .await
        .expect("save")
    }

    async fn set_last_used(db: &Db, id: Uuid, ts: DateTime<Utc>) {
        sqlx::query("UPDATE memories SET last_used_at = ?1 WHERE id = ?2")
            .bind(ts)
            .bind(id)
            .execute(db.pool())
            .await
            .expect("set last_used_at");
    }

    #[tokio::test]
    async fn stale_lists_old_memories_oldest_first_and_skips_protected() {
        let db = setup().await;
        let older = save_at(&db, MemoryKind::Note, "Nota vieja", None, None, 200).await;
        let old = save_at(&db, MemoryKind::Decision, "Decisión vieja", None, None, 100).await;
        save_at(
            &db,
            MemoryKind::Preference,
            "Preferencia vieja",
            None,
            None,
            300,
        )
        .await;
        save_at(
            &db,
            MemoryKind::SessionSummary,
            "Resumen viejo",
            None,
            None,
            300,
        )
        .await;
        save_at(&db, MemoryKind::Note, "Nota reciente", None, None, 1).await;
        let pinned = save_at(&db, MemoryKind::Note, "Fijada vieja", None, None, 300).await;
        db.set_memory_pinned(pinned.id, true).await.expect("pin");

        let stale = db
            .stale_memories(MemoryScope::All, 60)
            .await
            .expect("stale");
        let ids: Vec<Uuid> = stale.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![older.id, old.id]);
    }

    #[tokio::test]
    async fn stale_uses_the_newest_of_updated_at_and_last_used_at() {
        let db = setup().await;
        // Old update but used recently: not stale.
        let used = save_at(&db, MemoryKind::Note, "Usada hace poco", None, None, 200).await;
        set_last_used(&db, used.id, days_ago(2)).await;
        // Recent update but last use long ago: still not stale.
        let edited = save_at(&db, MemoryKind::Note, "Editada hace poco", None, None, 1).await;
        set_last_used(&db, edited.id, days_ago(300)).await;
        // Both old: stale.
        let forgotten = save_at(&db, MemoryKind::Note, "Olvidada", None, None, 90).await;
        set_last_used(&db, forgotten.id, days_ago(80)).await;

        let stale = db
            .stale_memories(MemoryScope::All, 60)
            .await
            .expect("stale");
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, forgotten.id);
    }

    #[tokio::test]
    async fn stale_scope_project_sees_own_and_global_but_not_other_projects() {
        let db = setup().await;
        let mine = db
            .create_project(CreateProjectInput {
                name: "mio".into(),
                path: "/tmp/mio".into(),
                context_notes: None,
            })
            .await
            .expect("project");
        let other = db
            .create_project(CreateProjectInput {
                name: "otro".into(),
                path: "/tmp/otro".into(),
                context_notes: None,
            })
            .await
            .expect("project");
        let own = save_at(
            &db,
            MemoryKind::Note,
            "Del proyecto",
            Some(mine.id),
            None,
            100,
        )
        .await;
        let global = save_at(&db, MemoryKind::Note, "Global vieja", None, None, 100).await;
        save_at(
            &db,
            MemoryKind::Note,
            "De otro proyecto",
            Some(other.id),
            None,
            100,
        )
        .await;

        let scoped = db
            .stale_memories(MemoryScope::Project(mine.id), 60)
            .await
            .expect("scoped");
        let mut ids: Vec<Uuid> = scoped.iter().map(|m| m.id).collect();
        ids.sort();
        let mut expected = vec![own.id, global.id];
        expected.sort();
        assert_eq!(ids, expected);

        let globals = db
            .stale_memories(MemoryScope::GlobalOnly, 60)
            .await
            .expect("globals");
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].id, global.id);
    }

    #[tokio::test]
    async fn surplus_keeps_newest_per_project_and_global_group() {
        let db = setup().await;
        let project = db
            .create_project(CreateProjectInput {
                name: "resumenes".into(),
                path: "/tmp/resumenes".into(),
                context_notes: None,
            })
            .await
            .expect("project");
        let mut project_sessions = Vec::new();
        for age in [10, 20, 30, 40] {
            project_sessions.push(
                save_at(
                    &db,
                    MemoryKind::SessionSummary,
                    &format!("Sesión proyecto {age}"),
                    Some(project.id),
                    None,
                    age,
                )
                .await,
            );
        }
        let mut global_sessions = Vec::new();
        for age in [5, 15, 25] {
            global_sessions.push(
                save_at(
                    &db,
                    MemoryKind::SessionSummary,
                    &format!("Sesión global {age}"),
                    None,
                    None,
                    age,
                )
                .await,
            );
        }
        // Orphan folder names group on their own.
        save_at(
            &db,
            MemoryKind::SessionSummary,
            "Sesión huérfana 1",
            None,
            Some("importado"),
            50,
        )
        .await;
        save_at(
            &db,
            MemoryKind::SessionSummary,
            "Sesión huérfana 2",
            None,
            Some("importado"),
            60,
        )
        .await;

        let surplus = db
            .surplus_session_summaries(MemoryScope::All, 2)
            .await
            .expect("surplus");
        let ids: Vec<Uuid> = surplus.iter().map(|m| m.id).collect();
        // Oldest first: project keeps ages 10 and 20, global keeps 5 and 15,
        // the orphan group keeps both.
        assert_eq!(
            ids,
            vec![
                project_sessions[3].id,
                project_sessions[2].id,
                global_sessions[2].id,
            ]
        );

        let scoped = db
            .surplus_session_summaries(MemoryScope::Project(project.id), 2)
            .await
            .expect("scoped surplus");
        let scoped_ids: Vec<Uuid> = scoped.iter().map(|m| m.id).collect();
        assert_eq!(
            scoped_ids,
            vec![
                project_sessions[3].id,
                project_sessions[2].id,
                global_sessions[2].id
            ]
        );

        let none = db
            .surplus_session_summaries(MemoryScope::All, 10)
            .await
            .expect("keep all");
        assert_eq!(none.len(), 0);
    }

    #[tokio::test]
    async fn stale_report_combines_both_lists() {
        let db = setup().await;
        let old = save_at(&db, MemoryKind::Note, "Vieja", None, None, 100).await;
        let mut sessions = Vec::new();
        for age in [1, 2, 3] {
            sessions.push(
                save_at(
                    &db,
                    MemoryKind::SessionSummary,
                    &format!("Sesión {age}"),
                    None,
                    None,
                    age,
                )
                .await,
            );
        }

        let report = db
            .stale_report(MemoryScope::All, 60, 2)
            .await
            .expect("report");
        assert_eq!(report.stale.len(), 1);
        assert_eq!(report.stale[0].id, old.id);
        assert_eq!(report.surplus_sessions.len(), 1);
        assert_eq!(report.surplus_sessions[0].id, sessions[2].id);
    }
}
