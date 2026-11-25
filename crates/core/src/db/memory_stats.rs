use crate::db::Db;
use crate::error::CoreResult;
use crate::models::{AgentMemorySaves, MemoryUsageStats, MostUsedMemory, ProjectMemoryUsage};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

#[derive(sqlx::FromRow)]
struct AgentSaveRow {
    actor_name: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct TotalCountsRow {
    total: i64,
    never_used: i64,
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if the statistics cannot be computed.
    pub async fn memory_usage_stats(&self) -> CoreResult<MemoryUsageStats> {
        self.memory_usage_stats_at(Utc::now()).await
    }

    /// # Errors
    ///
    /// Returns an error if the statistics cannot be computed.
    pub async fn memory_usage_stats_at(&self, now: DateTime<Utc>) -> CoreResult<MemoryUsageStats> {
        let (saves_by_agent, most_used, totals, by_project) = tokio::try_join!(
            self.fetch_saves_by_agent(now),
            self.fetch_most_used(),
            self.fetch_totals(),
            self.fetch_by_project(),
        )?;

        Ok(MemoryUsageStats {
            saves_by_agent,
            most_used,
            never_used: totals.never_used,
            total: totals.total,
            by_project,
        })
    }

    async fn fetch_saves_by_agent(&self, now: DateTime<Utc>) -> CoreResult<Vec<AgentMemorySaves>> {
        let cutoff_30d = now - Duration::days(30);
        let cutoff_7d = now - Duration::days(7);
        let sql_cutoff = cutoff_30d - Duration::seconds(1);

        let rows = sqlx::query_as::<_, AgentSaveRow>(
            r"
            SELECT
                COALESCE(NULLIF(TRIM(actor_name), ''), 'agente') AS actor_name,
                created_at
            FROM activity_events
            WHERE entity = 'memory'
              AND action IN ('create', 'update')
              AND actor_kind = 'agent'
              AND julianday(created_at) >= julianday(?1)
            ",
        )
        .bind(sql_cutoff)
        .fetch_all(&self.pool)
        .await?;

        let mut counts: HashMap<String, (i64, i64)> = HashMap::new();
        for row in rows {
            if row.created_at < cutoff_30d {
                continue;
            }
            let entry = counts.entry(row.actor_name).or_insert((0, 0));
            entry.1 += 1;
            if row.created_at >= cutoff_7d {
                entry.0 += 1;
            }
        }

        let mut result: Vec<AgentMemorySaves> = counts
            .into_iter()
            .map(|(actor_name, (last_7d, last_30d))| AgentMemorySaves {
                actor_name,
                last_7d,
                last_30d,
            })
            .collect();
        result.sort_by(|a, b| {
            b.last_30d
                .cmp(&a.last_30d)
                .then_with(|| b.last_7d.cmp(&a.last_7d))
                .then_with(|| a.actor_name.cmp(&b.actor_name))
        });
        Ok(result)
    }

    async fn fetch_most_used(&self) -> CoreResult<Vec<MostUsedMemory>> {
        let items = sqlx::query_as::<_, MostUsedMemory>(
            r"
            SELECT
                m.id,
                m.title,
                COALESCE(NULLIF(p.name, ''), NULLIF(m.project_name, '')) AS project_name,
                m.use_count,
                m.last_used_at
            FROM memories m
            LEFT JOIN projects p ON p.id = m.project_id
            WHERE m.use_count > 0
            ORDER BY m.use_count DESC, m.last_used_at DESC, m.updated_at DESC
            LIMIT 5
            ",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(items)
    }

    async fn fetch_totals(&self) -> CoreResult<TotalCountsRow> {
        let row = sqlx::query_as::<_, TotalCountsRow>(
            r"
            SELECT
                COUNT(*) AS total,
                COALESCE(
                    SUM(CASE WHEN use_count = 0 AND kind <> 'preference' THEN 1 ELSE 0 END),
                    0
                ) AS never_used
            FROM memories
            ",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn fetch_by_project(&self) -> CoreResult<Vec<ProjectMemoryUsage>> {
        let rows = sqlx::query_as::<_, ProjectMemoryUsage>(
            r"
            SELECT
                COALESCE(NULLIF(p.name, ''), NULLIF(m.project_name, ''), 'global') AS project_name,
                COUNT(*) AS total,
                COALESCE(SUM(CASE WHEN m.use_count > 0 THEN 1 ELSE 0 END), 0) AS used
            FROM memories m
            LEFT JOIN projects p ON p.id = m.project_id
            -- One row per registered project (by id, so same-name projects stay
            -- apart), then one per orphan folder name, then the global bucket.
            GROUP BY m.project_id, CASE WHEN m.project_id IS NULL THEN m.project_name END
            ORDER BY total DESC, used DESC, project_name ASC
            ",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateProjectInput, MemoryKind, SaveMemoryInput};
    use uuid::Uuid;

    async fn setup() -> Db {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");
        db
    }

    async fn insert_activity(
        db: &Db,
        actor_kind: &str,
        actor_name: Option<&str>,
        action: &str,
        entity: &str,
        created_at: DateTime<Utc>,
    ) {
        sqlx::query(
            r"
            INSERT INTO activity_events (id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at)
            VALUES (?1, NULL, ?2, ?3, ?4, ?5, NULL, NULL, ?6)
            ",
        )
        .bind(Uuid::now_v7())
        .bind(actor_kind)
        .bind(actor_name)
        .bind(action)
        .bind(entity)
        .bind(created_at)
        .execute(db.pool())
        .await
        .expect("insert activity");
    }

    #[tokio::test]
    async fn empty_db_returns_zero_stats() {
        let db = setup().await;
        let stats = db.memory_usage_stats().await.expect("stats");
        assert_eq!(stats.total, 0);
        assert_eq!(stats.never_used, 0);
        assert_eq!(stats.saves_by_agent, Vec::new());
        assert_eq!(stats.most_used, Vec::new());
        assert_eq!(stats.by_project, Vec::new());
    }

    async fn seed_project(db: &Db, name: &str) -> crate::models::Project {
        db.create_project(CreateProjectInput {
            name: name.into(),
            path: format!("/tmp/{name}"),
            context_notes: None,
        })
        .await
        .expect("project")
    }

    async fn seed_memory(
        db: &Db,
        project_id: Option<Uuid>,
        project_name: Option<&str>,
        kind: MemoryKind,
        title: &str,
    ) -> Uuid {
        db.save_memory(SaveMemoryInput {
            project_id,
            project_name: project_name.map(str::to_string),
            kind,
            title: title.into(),
            content: "Contenido".into(),
            topic_key: None,
        })
        .await
        .expect("save memory")
        .id
    }

    #[tokio::test]
    async fn stats_aggregate_most_used_never_used_and_by_project() {
        let db = setup().await;
        let alpha = seed_project(&db, "alpha").await;
        let beta = seed_project(&db, "beta").await;

        let mut alpha_ids = Vec::new();
        for i in 0..6 {
            let title = format!("Alpha {i}");
            let id = seed_memory(&db, Some(alpha.id), None, MemoryKind::Decision, &title).await;
            alpha_ids.push(id);
        }
        let beta_used =
            seed_memory(&db, Some(beta.id), None, MemoryKind::Pattern, "Beta used").await;
        seed_memory(&db, Some(beta.id), None, MemoryKind::Bugfix, "Beta unused").await;
        seed_memory(
            &db,
            None,
            None,
            MemoryKind::Preference,
            "Preferencia usuario",
        )
        .await;

        for _ in 0..4 {
            db.touch_memories(&[alpha_ids[0]]).await.expect("touch");
        }
        for _ in 0..3 {
            db.touch_memories(&[beta_used]).await.expect("touch");
        }
        for _ in 0..2 {
            db.touch_memories(&[alpha_ids[1]]).await.expect("touch");
        }
        db.touch_memories(&[alpha_ids[2], alpha_ids[3], alpha_ids[4]])
            .await
            .expect("touch");

        let stats = db.memory_usage_stats().await.expect("stats");
        assert_eq!(stats.total, 9);
        assert_eq!(stats.never_used, 2);
        assert_eq!(stats.most_used.len(), 5);
        assert_eq!(stats.most_used[0].id, alpha_ids[0]);
        assert_eq!(stats.most_used[0].use_count, 4);
        assert_eq!(stats.most_used[0].project_name.as_deref(), Some("alpha"));
        assert!(stats.most_used[0].last_used_at.is_some());
        assert_eq!(stats.most_used[1].id, beta_used);
        assert_eq!(stats.most_used[1].use_count, 3);
        assert_eq!(stats.most_used[1].project_name.as_deref(), Some("beta"));
        assert_eq!(stats.most_used[2].id, alpha_ids[1]);
        assert_eq!(stats.most_used[2].use_count, 2);

        assert_eq!(
            stats.by_project,
            vec![
                ProjectMemoryUsage {
                    project_name: "alpha".into(),
                    total: 6,
                    used: 5,
                },
                ProjectMemoryUsage {
                    project_name: "beta".into(),
                    total: 2,
                    used: 1,
                },
                ProjectMemoryUsage {
                    project_name: "global".into(),
                    total: 1,
                    used: 0,
                },
            ]
        );
    }

    #[tokio::test]
    async fn stats_aggregate_agent_saves_in_7d_and_30d_windows() {
        let db = setup().await;
        let now = Utc::now();

        let events = [
            (
                "agent",
                Some("claude"),
                "create",
                "memory",
                now - Duration::days(2),
            ),
            (
                "agent",
                Some("claude"),
                "update",
                "memory",
                now - Duration::days(10),
            ),
            (
                "agent",
                Some("claude"),
                "create",
                "memory",
                now - Duration::days(40),
            ),
            (
                "agent",
                Some("opencode"),
                "create",
                "memory",
                now - Duration::hours(5),
            ),
            (
                "agent",
                Some("opencode"),
                "create",
                "memory",
                now - Duration::days(1),
            ),
            (
                "agent",
                Some("opencode"),
                "delete",
                "memory",
                now - Duration::hours(1),
            ),
            (
                "agent",
                Some("opencode"),
                "create",
                "task",
                now - Duration::hours(1),
            ),
            ("user", None, "create", "memory", now - Duration::hours(1)),
        ];
        for (actor_kind, actor_name, action, entity, created_at) in events {
            insert_activity(&db, actor_kind, actor_name, action, entity, created_at).await;
        }

        let stats = db.memory_usage_stats_at(now).await.expect("stats");
        assert_eq!(
            stats.saves_by_agent,
            vec![
                AgentMemorySaves {
                    actor_name: "opencode".into(),
                    last_7d: 2,
                    last_30d: 2,
                },
                AgentMemorySaves {
                    actor_name: "claude".into(),
                    last_7d: 1,
                    last_30d: 2,
                },
            ]
        );
    }
}
