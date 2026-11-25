use crate::db::Db;
use crate::error::CoreResult;
use crate::models::{MemoryKind, SaveMemoryInput};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportReport {
    pub read: usize,
    pub imported: usize,
    pub linked_to_projects: usize,
    pub skipped: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct Observation {
    id: i64,
    kind: String,
    title: String,
    content: String,
    project: Option<String>,
    scope: String,
    topic_key: Option<String>,
    created_at: String,
    updated_at: String,
}

pub(crate) fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|t| t.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            // engram writes "2026-11-19 20:53:03" and "2026-11-19 20:53:03.723582085"
            NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|t| t.and_utc())
        })
}

impl Db {
    pub async fn import_engram(&self, engram_db: &Path) -> CoreResult<ImportReport> {
        let options = SqliteConnectOptions::new()
            .filename(engram_db)
            .read_only(true);
        let source = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let rows = sqlx::query_as::<_, Observation>(
            r"
            SELECT id, type AS kind, title, content, project, scope, topic_key, created_at, updated_at
            FROM observations
            WHERE deleted_at IS NULL
            ORDER BY id
            ",
        )
        .fetch_all(&source)
        .await?;
        source.close().await;

        let projects: HashMap<String, uuid::Uuid> = self
            .list_projects()
            .await?
            .into_iter()
            .map(|p| (p.name.to_lowercase(), p.id))
            .collect();

        let mut report = ImportReport {
            read: rows.len(),
            ..ImportReport::default()
        };
        for row in rows {
            let global = row.scope != "project";
            let name = if global {
                None
            } else {
                row.project.clone().filter(|p| !p.trim().is_empty())
            };
            let project_id = name
                .as_deref()
                .and_then(|n| projects.get(&n.to_lowercase()).copied());
            let kind = MemoryKind::parse(&row.kind).unwrap_or(MemoryKind::Note);
            let topic = row
                .topic_key
                .clone()
                .filter(|t| !t.trim().is_empty())
                // Stable and non-empty after topic_key normalization, so
                // re-importing upserts instead of duplicating.
                .unwrap_or_else(|| format!("engram-obs-{}", row.id));
            let updated = parse_time(&row.updated_at).unwrap_or_else(Utc::now);
            let created = parse_time(&row.created_at);
            let input = SaveMemoryInput {
                project_id,
                project_name: if project_id.is_some() { None } else { name },
                kind,
                title: row.title.chars().take(200).collect(),
                content: row.content.chars().take(20_000).collect(),
                topic_key: Some(topic.chars().take(120).collect()),
            };
            match self.save_memory_at(input, updated, created).await {
                Ok(_) => {
                    report.imported += 1;
                    if project_id.is_some() {
                        report.linked_to_projects += 1;
                    }
                }
                Err(err) => report.skipped.push(format!("#{}: {err}", row.id)),
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_time;

    #[test]
    fn parses_engram_timestamps_with_and_without_fraction() {
        let plain = parse_time("2026-11-19 20:53:03").expect("plain");
        let fraction = parse_time("2026-11-19 20:53:03.723582085").expect("fraction");
        assert_eq!(plain.timestamp(), fraction.timestamp());
        assert!(parse_time("2026-11-19T20:53:03+00:00").is_some());
        assert!(parse_time("ayer").is_none());
    }
}
