//! Git sync without a server: `export` writes the project's shareable
//! knowledge to `.saispace/sync/*.jsonl.gz` (sorted, so diffs stay small)
//! and `import` merges it back, newest `updated_at` wins.
//!
//! Only `scope = project` memories of that project travel, with their links,
//! triggers and the tech rules of its stack. Personal and global memories and
//! the vault never leave the machine.

use crate::db::Db;
use crate::db::memory::MEMORY_COLUMNS;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, MemoryKind, Project, UpsertTechRuleInput};
use chrono::{DateTime, Utc};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const SYNC_DIR: &str = ".saispace/sync";
const MEMORIES: &str = "memories.jsonl.gz";
const LINKS: &str = "links.jsonl.gz";
const TRIGGERS: &str = "triggers.jsonl.gz";
const TECH_RULES: &str = "tech_rules.jsonl.gz";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncMemory {
    pub id: Uuid,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_key: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct SyncLink {
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct SyncTrigger {
    pub memory_id: Uuid,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncTechRule {
    pub technology: String,
    #[serde(default)]
    pub version: String,
    pub rule: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct SyncCounts {
    pub memories: usize,
    pub links: usize,
    pub triggers: usize,
    pub tech_rules: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub links: u64,
    pub triggers: u64,
    pub tech_rules: usize,
}

fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> CoreResult<()> {
    let io = |e: std::io::Error| CoreError::Validation(format!("{}: {e}", path.display()));
    // Default gzip header has mtime 0: same data, same bytes.
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    for row in rows {
        serde_json::to_writer(&mut gz, row)?;
        gz.write_all(b"\n").map_err(io)?;
    }
    let bytes = gz.finish().map_err(io)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(io)?;
    fs::rename(&tmp, path).map_err(io)?;
    Ok(())
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> CoreResult<Vec<T>> {
    let Ok(file) = fs::File::open(path) else {
        return Ok(Vec::new());
    };
    let mut rows = Vec::new();
    for line in BufReader::new(GzDecoder::new(file)).lines() {
        let line = line.map_err(|e| CoreError::Validation(format!("{}: {e}", path.display())))?;
        if !line.trim().is_empty() {
            rows.push(serde_json::from_str(&line)?);
        }
    }
    Ok(rows)
}

#[must_use]
pub fn sync_dir(project: &Project) -> PathBuf {
    Path::new(&project.path).join(SYNC_DIR)
}

impl Db {
    /// Adds or updates one memory; returns its local id.
    async fn import_memory(
        &self,
        project: &Project,
        m: &SyncMemory,
        report: &mut ImportReport,
    ) -> CoreResult<Uuid> {
        let title = self.scrub_secrets(&m.title).await?;
        let content = self.scrub_secrets(&m.content).await?;
        let topic = m.topic_key.clone().unwrap_or_default();
        let existing: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, updated_at FROM memories
             WHERE id = ?1 OR (?2 <> '' AND project_name = ?3 AND topic_key = ?2)
             ORDER BY id = ?1 DESC LIMIT 1",
        )
        .bind(m.id)
        .bind(&topic)
        .bind(&project.name)
        .fetch_optional(self.pool())
        .await?;
        match existing {
            Some((id, local)) if local >= m.updated_at => {
                report.unchanged += 1;
                Ok(id)
            }
            Some((id, _)) => {
                sqlx::query(
                    "UPDATE memories SET kind = ?2, title = ?3, content = ?4, pinned = ?5,
                            updated_at = ?6
                     WHERE id = ?1",
                )
                .bind(id)
                .bind(m.kind)
                .bind(&title)
                .bind(&content)
                .bind(m.pinned)
                .bind(m.updated_at)
                .execute(self.pool())
                .await?;
                report.updated += 1;
                Ok(id)
            }
            None => {
                sqlx::query(
                    "INSERT INTO memories (id, project_id, project_name, kind, title, content,
                            topic_key, pinned, created_at, updated_at, scope)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'project')",
                )
                .bind(m.id)
                .bind(project.id)
                .bind(&project.name)
                .bind(m.kind)
                .bind(&title)
                .bind(&content)
                .bind(&topic)
                .bind(m.pinned)
                .bind(m.created_at)
                .bind(m.updated_at)
                .execute(self.pool())
                .await?;
                report.added += 1;
                Ok(m.id)
            }
        }
    }

    async fn shareable_memories(&self, project_id: Uuid) -> CoreResult<Vec<Memory>> {
        let sql = format!(
            "SELECT {MEMORY_COLUMNS} FROM memories
             WHERE project_id = ?1 AND scope = 'project'
             ORDER BY id"
        );
        Ok(sqlx::query_as::<_, Memory>(&sql)
            .bind(project_id)
            .fetch_all(self.pool())
            .await?)
    }

    /// Writes the project's shareable knowledge under `dir`.
    ///
    /// # Errors
    ///
    /// Returns an error if the data cannot be read or the files written.
    pub async fn sync_export(&self, project: &Project, dir: &Path) -> CoreResult<SyncCounts> {
        fs::create_dir_all(dir)
            .map_err(|e| CoreError::Validation(format!("{}: {e}", dir.display())))?;
        let memories = self.shareable_memories(project.id).await?;
        let ids: std::collections::HashSet<Uuid> = memories.iter().map(|m| m.id).collect();
        let rows: Vec<SyncMemory> = memories
            .into_iter()
            .map(|m| SyncMemory {
                id: m.id,
                kind: m.kind,
                title: m.title,
                content: m.content,
                topic_key: m.topic_key,
                pinned: m.pinned,
                created_at: m.created_at,
                updated_at: m.updated_at,
                superseded_by: m.superseded_by.filter(|s| ids.contains(s)),
            })
            .collect();

        let links: Vec<SyncLink> = sqlx::query_as(
            "SELECT from_id, to_id, relation FROM memory_links ORDER BY from_id, to_id, relation",
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .filter(|l: &SyncLink| ids.contains(&l.from_id) && ids.contains(&l.to_id))
        .collect();
        let triggers: Vec<SyncTrigger> = sqlx::query_as(
            "SELECT memory_id, pattern FROM memory_triggers ORDER BY memory_id, pattern",
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .filter(|t: &SyncTrigger| ids.contains(&t.memory_id))
        .collect();
        let mut rules: Vec<SyncTechRule> = self
            .tech_rules_for_project(project.id)
            .await?
            .into_iter()
            .map(|r| SyncTechRule {
                technology: r.technology,
                version: r.version_req.unwrap_or_default(),
                rule: r.rule,
                updated_at: r.updated_at,
            })
            .collect();
        rules.sort_by(|a, b| (&a.technology, &a.version).cmp(&(&b.technology, &b.version)));

        write_jsonl(&dir.join(MEMORIES), &rows)?;
        write_jsonl(&dir.join(LINKS), &links)?;
        write_jsonl(&dir.join(TRIGGERS), &triggers)?;
        write_jsonl(&dir.join(TECH_RULES), &rules)?;
        Ok(SyncCounts {
            memories: rows.len(),
            links: links.len(),
            triggers: triggers.len(),
            tech_rules: rules.len(),
        })
    }

    /// Merges the files under `dir` into `project`. Running it twice changes
    /// nothing the second time; a local edit newer than the file is kept.
    ///
    /// # Errors
    ///
    /// Returns an error if a file is malformed or a write fails.
    pub async fn sync_import(&self, project: &Project, dir: &Path) -> CoreResult<ImportReport> {
        let memories: Vec<SyncMemory> = read_jsonl(&dir.join(MEMORIES))?;
        let links: Vec<SyncLink> = read_jsonl(&dir.join(LINKS))?;
        let triggers: Vec<SyncTrigger> = read_jsonl(&dir.join(TRIGGERS))?;
        let rules: Vec<SyncTechRule> = read_jsonl(&dir.join(TECH_RULES))?;
        let mut report = ImportReport::default();
        // Incoming id -> local id (differs when matched by topic_key).
        let mut local_ids = std::collections::HashMap::new();

        for m in &memories {
            let id = self.import_memory(project, m, &mut report).await?;
            local_ids.insert(m.id, id);
        }
        for m in &memories {
            if let (Some(id), Some(target)) = (
                local_ids.get(&m.id),
                m.superseded_by.and_then(|s| local_ids.get(&s)),
            ) {
                sqlx::query(
                    "UPDATE memories SET superseded_by = ?2 WHERE id = ?1 AND superseded_by IS NULL",
                )
                .bind(id)
                .bind(target)
                .execute(self.pool())
                .await?;
            }
        }
        let now = Utc::now();
        for l in &links {
            let (Some(from), Some(to)) = (local_ids.get(&l.from_id), local_ids.get(&l.to_id))
            else {
                continue;
            };
            report.links += sqlx::query(
                "INSERT OR IGNORE INTO memory_links (from_id, to_id, relation, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(from)
            .bind(to)
            .bind(&l.relation)
            .bind(now)
            .execute(self.pool())
            .await?
            .rows_affected();
        }
        for t in &triggers {
            let Some(id) = local_ids.get(&t.memory_id) else {
                continue;
            };
            report.triggers += sqlx::query(
                "INSERT OR IGNORE INTO memory_triggers (memory_id, pattern, created_at)
                 VALUES (?1, ?2, ?3)",
            )
            .bind(id)
            .bind(&t.pattern)
            .bind(now)
            .execute(self.pool())
            .await?
            .rows_affected();
        }
        for r in rules {
            let local: Option<DateTime<Utc>> = sqlx::query_scalar(
                "SELECT updated_at FROM tech_rules WHERE technology = ?1 AND version_req = ?2",
            )
            .bind(r.technology.to_lowercase())
            .bind(&r.version)
            .fetch_optional(self.pool())
            .await?;
            if local.is_some_and(|at| at >= r.updated_at) {
                continue;
            }
            self.upsert_tech_rule(UpsertTechRuleInput {
                technology: r.technology,
                version: Some(r.version).filter(|v| !v.is_empty()),
                rule: r.rule,
            })
            .await?;
            report.tech_rules += 1;
        }
        Ok(report)
    }
}
