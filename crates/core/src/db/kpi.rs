use crate::db::Db;
use crate::db::memory_stale::DEFAULT_STALE_DAYS;
use crate::error::CoreResult;
use crate::models::{
    ActionKind, EntityKind, KpiComputation, KpiHistory, KpiMetricDef, KpiPoint, KpiScopeSnapshot,
    KpiSeries, LearningKpis, MemoryKind, MemoryScope, Project,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const KPI_CATALOG: &[KpiMetricDef] = &[
    KpiMetricDef {
        name: "memories_active",
        label: "Memorias activas",
        unit: "count",
        higher_is_better: None,
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "memories_archived",
        label: "Memorias archivadas",
        unit: "count",
        higher_is_better: None,
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "preferences_active",
        label: "Preferencias activas",
        unit: "count",
        higher_is_better: None,
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "duplicate_pairs",
        label: "Pares duplicados",
        unit: "count",
        higher_is_better: Some(false),
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "stale_memories",
        label: "Memorias obsoletas",
        unit: "count",
        higher_is_better: Some(false),
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "link_density",
        label: "Densidad de enlaces",
        unit: "links_per_memory",
        higher_is_better: Some(true),
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "topic_key_ratio",
        label: "Uso de topic_key",
        unit: "ratio",
        higher_is_better: Some(true),
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "reuse_rate_30d",
        label: "Tasa de reuso (30d)",
        unit: "ratio",
        higher_is_better: Some(true),
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "saves_7d",
        label: "Guardados (7d)",
        unit: "count",
        higher_is_better: None,
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "agents_active_7d",
        label: "Agentes activos (7d)",
        unit: "count",
        higher_is_better: None,
        global_only: false,
        project_only: false,
    },
    KpiMetricDef {
        name: "index_bytes",
        label: "Tamaño del índice",
        unit: "bytes",
        higher_is_better: Some(false),
        global_only: false,
        project_only: true,
    },
    KpiMetricDef {
        name: "corrections_7d",
        label: "Correcciones (7d)",
        unit: "count",
        higher_is_better: Some(false),
        global_only: true,
        project_only: false,
    },
    KpiMetricDef {
        name: "stop_blocks_7d",
        label: "Bloqueos en Stop (7d)",
        unit: "count",
        higher_is_better: Some(false),
        global_only: true,
        project_only: false,
    },
    KpiMetricDef {
        name: "recalls_7d",
        label: "Lecciones recordadas por error (7d)",
        unit: "count",
        higher_is_better: None,
        global_only: true,
        project_only: false,
    },
    KpiMetricDef {
        name: "lesson_conversion_7d",
        label: "Conversión a lección (7d)",
        unit: "ratio",
        higher_is_better: Some(true),
        global_only: true,
        project_only: false,
    },
];

#[must_use]
pub fn kpi_catalog() -> &'static [KpiMetricDef] {
    KPI_CATALOG
}

#[must_use]
pub fn metric_applies_to_scope(def: &KpiMetricDef, scope: &str) -> bool {
    if scope == "global" {
        !def.project_only
    } else {
        !def.global_only
    }
}

#[derive(sqlx::FromRow)]
struct MemoryMetricRow {
    project_id: Option<Uuid>,
    kind: MemoryKind,
    topic_key: String,
    last_used_at: Option<DateTime<Utc>>,
    superseded_by: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct LinkScopeRow {
    from_project_id: Option<Uuid>,
    to_project_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct ActivityMetricRow {
    project_id: Option<Uuid>,
    actor_name: String,
    action: ActionKind,
    entity: EntityKind,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct SnapshotPointRow {
    day: String,
    metric: String,
    value: f64,
}

#[derive(sqlx::FromRow)]
struct BaselineRow {
    metric: String,
    value: f64,
}

#[derive(Default)]
struct ScopeAccumulator {
    active: usize,
    archived: usize,
    preferences_active: usize,
    with_topic_key: usize,
    reused_30d: usize,
    links: usize,
    duplicates: usize,
    stale: usize,
    saves_7d: usize,
    agents_7d: HashSet<String>,
    index_bytes: Option<usize>,
}

impl ScopeAccumulator {
    fn to_metrics_map(&self) -> BTreeMap<String, f64> {
        #[allow(clippy::cast_precision_loss)]
        let ratio = |num: usize, den: usize| -> f64 {
            if den == 0 {
                0.0
            } else {
                num as f64 / den as f64
            }
        };
        #[allow(clippy::cast_precision_loss)]
        let mut map = BTreeMap::from([
            ("memories_active".to_string(), self.active as f64),
            ("memories_archived".to_string(), self.archived as f64),
            (
                "preferences_active".to_string(),
                self.preferences_active as f64,
            ),
            ("duplicate_pairs".to_string(), self.duplicates as f64),
            ("stale_memories".to_string(), self.stale as f64),
            ("link_density".to_string(), ratio(self.links, self.active)),
            (
                "topic_key_ratio".to_string(),
                ratio(self.with_topic_key, self.active),
            ),
            (
                "reuse_rate_30d".to_string(),
                ratio(self.reused_30d, self.active),
            ),
            ("saves_7d".to_string(), self.saves_7d as f64),
            ("agents_active_7d".to_string(), self.agents_7d.len() as f64),
        ]);
        if let Some(bytes) = self.index_bytes {
            #[allow(clippy::cast_precision_loss)]
            map.insert("index_bytes".to_string(), bytes as f64);
        }
        map
    }
}

#[derive(Deserialize)]
struct RawKpiEvent {
    ts: DateTime<Utc>,
    event: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    corrections: Option<usize>,
    #[serde(default)]
    saved: Option<bool>,
    #[serde(default)]
    blocked: Option<bool>,
}

fn parse_rotated_ms(name: &str) -> Option<u128> {
    let rest = name.strip_prefix("kpi-events.")?.strip_suffix(".jsonl")?;
    rest.split('.').next()?.parse::<u128>().ok()
}

fn collect_event_log_paths(log_path: &Path) -> CoreResult<Vec<PathBuf>> {
    let Some(parent) = log_path.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Ok(vec![log_path.to_path_buf()]);
    };
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(crate::error::CoreError::Validation(format!(
                "cannot read {}: {err}",
                parent.display()
            )));
        }
    };
    let main_name = log_path.file_name().and_then(|n| n.to_str());
    let mut rotated: Vec<(bool, u128, String, PathBuf)> = Vec::new();
    let mut has_main = false;

    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if Some(name.as_str()) == main_name {
            has_main = true;
            continue;
        }
        let is_rotated_jsonl = name.starts_with("kpi-events")
            && Path::new(&name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"));
        let is_legacy = name == "kpi-events.jsonl.1";
        if is_rotated_jsonl || is_legacy {
            let ms = parse_rotated_ms(&name);
            rotated.push((ms.is_none(), ms.unwrap_or(0), name, path));
        }
    }

    rotated.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    let mut paths: Vec<PathBuf> = rotated.into_iter().map(|(_, _, _, p)| p).collect();
    if has_main || log_path.is_file() {
        paths.push(log_path.to_path_buf());
    }
    Ok(paths)
}

/// # Errors
///
/// Returns an error if an existing event log file cannot be opened.
pub fn compute_learning_kpis(log_path: &Path, now: DateTime<Utc>) -> CoreResult<LearningKpis> {
    let cutoff_7d = now - Duration::days(7);
    let mut corrections_7d: usize = 0;
    let mut stop_blocks_7d: usize = 0;
    let mut recalls_7d: usize = 0;
    let mut last_stop_by_session: HashMap<String, (DateTime<Utc>, usize, bool)> = HashMap::new();

    for path in collect_event_log_paths(log_path)? {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(crate::error::CoreError::Validation(format!(
                    "cannot read {}: {err}",
                    path.display()
                )));
            }
        };
        let reader = BufReader::new(file);
        for line_res in reader.lines() {
            let Ok(line) = line_res else {
                continue;
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(ev) = serde_json::from_str::<RawKpiEvent>(trimmed) else {
                continue;
            };
            if ev.ts < cutoff_7d {
                continue;
            }
            match ev.event.as_str() {
                "correction" => corrections_7d += 1,
                "recall" => recalls_7d += 1,
                "stop" => {
                    if ev.blocked == Some(true) {
                        stop_blocks_7d += 1;
                    }
                    if let Some(sid) = ev.session_id.filter(|s| !s.is_empty()) {
                        let corrections = ev.corrections.unwrap_or(0);
                        let saved = ev.saved.unwrap_or(false);
                        let replace = last_stop_by_session
                            .get(&sid)
                            .is_none_or(|(prev_ts, _, _)| *prev_ts <= ev.ts);
                        if replace {
                            last_stop_by_session.insert(sid, (ev.ts, corrections, saved));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let mut correction_sessions = 0_usize;
    let mut converted_sessions = 0_usize;
    for (_, corrections, saved) in last_stop_by_session.values() {
        if *corrections > 0 {
            correction_sessions += 1;
            if *saved {
                converted_sessions += 1;
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let lesson_conversion_7d = if correction_sessions == 0 {
        None
    } else {
        Some(converted_sessions as f64 / correction_sessions as f64)
    };

    #[allow(clippy::cast_precision_loss)]
    Ok(LearningKpis {
        corrections_7d: corrections_7d as f64,
        stop_blocks_7d: stop_blocks_7d as f64,
        lesson_conversion_7d,
        recalls_7d: recalls_7d as f64,
    })
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if the database queries fail.
    pub async fn compute_kpis(&self, now: DateTime<Utc>) -> CoreResult<KpiComputation> {
        let projects = self.list_projects().await?;
        let mut global = ScopeAccumulator::default();
        let mut per_project: HashMap<Uuid, ScopeAccumulator> = projects
            .iter()
            .map(|p| (p.id, ScopeAccumulator::default()))
            .collect();

        self.accumulate_memory_metrics(now, &mut global, &mut per_project)
            .await?;
        self.accumulate_link_metrics(&mut global, &mut per_project)
            .await?;
        self.accumulate_activity_metrics(now, &mut global, &mut per_project)
            .await?;
        self.accumulate_stale_metrics(now, &mut global, &mut per_project)
            .await?;
        let per_project_dupes = self
            .accumulate_duplicate_metrics(&projects, &mut global, &mut per_project)
            .await?;
        self.accumulate_index_bytes(&projects, &per_project_dupes, &mut per_project)
            .await?;

        let project_snapshots = projects
            .into_iter()
            .map(|p| {
                let metrics = per_project
                    .remove(&p.id)
                    .map_or_default(|acc| acc.to_metrics_map());
                KpiScopeSnapshot {
                    scope: p.id.to_string(),
                    metrics,
                }
            })
            .collect();

        Ok(KpiComputation {
            day: now.format("%Y-%m-%d").to_string(),
            computed_at: now,
            global: KpiScopeSnapshot {
                scope: "global".to_string(),
                metrics: global.to_metrics_map(),
            },
            projects: project_snapshots,
        })
    }

    async fn accumulate_memory_metrics(
        &self,
        now: DateTime<Utc>,
        global: &mut ScopeAccumulator,
        per_project: &mut HashMap<Uuid, ScopeAccumulator>,
    ) -> CoreResult<()> {
        let cutoff_30d = now - Duration::days(30);
        let rows = sqlx::query_as::<_, MemoryMetricRow>(
            "SELECT project_id, kind, topic_key, last_used_at, superseded_by FROM memories",
        )
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            if row.superseded_by.is_some() {
                global.archived += 1;
                if let Some(pid) = row.project_id
                    && let Some(acc) = per_project.get_mut(&pid)
                {
                    acc.archived += 1;
                }
                continue;
            }
            let is_pref = row.kind == MemoryKind::Preference;
            let has_topic = !row.topic_key.is_empty();
            let reused = row.last_used_at.is_some_and(|ts| ts >= cutoff_30d);

            global.active += 1;
            if is_pref {
                global.preferences_active += 1;
            }
            if has_topic {
                global.with_topic_key += 1;
            }
            if reused {
                global.reused_30d += 1;
            }

            if let Some(pid) = row.project_id
                && let Some(acc) = per_project.get_mut(&pid)
            {
                acc.active += 1;
                if is_pref {
                    acc.preferences_active += 1;
                }
                if has_topic {
                    acc.with_topic_key += 1;
                }
                if reused {
                    acc.reused_30d += 1;
                }
            }
        }
        Ok(())
    }

    async fn accumulate_link_metrics(
        &self,
        global: &mut ScopeAccumulator,
        per_project: &mut HashMap<Uuid, ScopeAccumulator>,
    ) -> CoreResult<()> {
        let rows = sqlx::query_as::<_, LinkScopeRow>(
            r"
            SELECT a.project_id AS from_project_id, b.project_id AS to_project_id
            FROM memory_links ml
            JOIN memories a ON a.id = ml.from_id AND a.superseded_by IS NULL
            JOIN memories b ON b.id = ml.to_id AND b.superseded_by IS NULL
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        global.links = rows.len();
        for row in rows {
            if let Some(from_pid) = row.from_project_id
                && let Some(acc) = per_project.get_mut(&from_pid)
            {
                acc.links += 1;
            }
            if let Some(to_pid) = row.to_project_id
                && Some(to_pid) != row.from_project_id
                && let Some(acc) = per_project.get_mut(&to_pid)
            {
                acc.links += 1;
            }
        }
        Ok(())
    }

    async fn accumulate_activity_metrics(
        &self,
        now: DateTime<Utc>,
        global: &mut ScopeAccumulator,
        per_project: &mut HashMap<Uuid, ScopeAccumulator>,
    ) -> CoreResult<()> {
        let cutoff_7d = now - Duration::days(7);
        let sql_cutoff = cutoff_7d - Duration::seconds(1);
        let rows = sqlx::query_as::<_, ActivityMetricRow>(
            r"
            SELECT
                project_id,
                COALESCE(NULLIF(TRIM(actor_name), ''), 'agente') AS actor_name,
                action,
                entity,
                created_at
            FROM activity_events
            WHERE actor_kind = 'agent'
              AND julianday(created_at) >= julianday(?1)
            ",
        )
        .bind(sql_cutoff)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            if row.created_at < cutoff_7d {
                continue;
            }
            let is_save = row.entity == EntityKind::Memory
                && matches!(row.action, ActionKind::Create | ActionKind::Update);
            if is_save {
                global.saves_7d += 1;
            }
            global.agents_7d.insert(row.actor_name.clone());

            if let Some(pid) = row.project_id
                && let Some(acc) = per_project.get_mut(&pid)
            {
                if is_save {
                    acc.saves_7d += 1;
                }
                acc.agents_7d.insert(row.actor_name);
            }
        }
        Ok(())
    }

    async fn accumulate_stale_metrics(
        &self,
        now: DateTime<Utc>,
        global: &mut ScopeAccumulator,
        per_project: &mut HashMap<Uuid, ScopeAccumulator>,
    ) -> CoreResult<()> {
        let stale = self
            .stale_memories_at(MemoryScope::All, DEFAULT_STALE_DAYS, now)
            .await?;
        global.stale = stale.len();
        for memory in stale {
            if let Some(pid) = memory.project_id
                && let Some(acc) = per_project.get_mut(&pid)
            {
                acc.stale += 1;
            }
        }
        Ok(())
    }

    async fn accumulate_duplicate_metrics(
        &self,
        projects: &[Project],
        global: &mut ScopeAccumulator,
        per_project: &mut HashMap<Uuid, ScopeAccumulator>,
    ) -> CoreResult<HashMap<Uuid, Vec<crate::db::DuplicatePair>>> {
        const DUPE_LIMIT: usize = 1_000;
        let global_dupes = self.duplicate_pairs(MemoryScope::All, DUPE_LIMIT).await?;
        global.duplicates = global_dupes.len();

        let mut per_project_dupes: HashMap<Uuid, Vec<crate::db::DuplicatePair>> =
            HashMap::with_capacity(projects.len());

        if global_dupes.len() < DUPE_LIMIT {
            for pair in &global_dupes {
                if let Some(pid) = pair.a.project_id
                    && let Some(acc) = per_project.get_mut(&pid)
                {
                    acc.duplicates += 1;
                }
            }
            for project in projects {
                let scoped: Vec<_> = global_dupes
                    .iter()
                    .filter(|pair| {
                        pair.a.project_id == Some(project.id)
                            || (pair.a.project_id.is_none()
                                && pair.a.project_name.as_deref().unwrap_or("").is_empty())
                    })
                    .cloned()
                    .collect();
                per_project_dupes.insert(project.id, scoped);
            }
            return Ok(per_project_dupes);
        }

        for project in projects {
            let pairs = self
                .duplicate_pairs(MemoryScope::Project(project.id), DUPE_LIMIT)
                .await?;
            let count = pairs
                .iter()
                .filter(|pair| pair.a.project_id == Some(project.id))
                .count();
            if let Some(acc) = per_project.get_mut(&project.id) {
                acc.duplicates = count;
            }
            per_project_dupes.insert(project.id, pairs);
        }
        Ok(per_project_dupes)
    }

    async fn accumulate_index_bytes(
        &self,
        projects: &[Project],
        per_project_dupes: &HashMap<Uuid, Vec<crate::db::DuplicatePair>>,
        per_project: &mut HashMap<Uuid, ScopeAccumulator>,
    ) -> CoreResult<()> {
        for project in projects {
            let dupes = per_project_dupes
                .get(&project.id)
                .map_or(&[][..], Vec::as_slice);
            let context = self
                .memory_context_with_dupes(MemoryScope::Project(project.id), 12, dupes)
                .await?;
            let rendered = crate::memory_render::render_context(Some(project), &context);
            if let Some(acc) = per_project.get_mut(&project.id) {
                acc.index_bytes = Some(rendered.len());
            }
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the snapshot check query fails.
    pub async fn has_snapshot_for_day(&self, day: &str, scope: &str) -> CoreResult<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM kpi_snapshots WHERE day = ?1 AND scope = ?2)",
        )
        .bind(day)
        .bind(scope)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// # Errors
    ///
    /// Returns an error if KPI computation, learning log parsing, or snapshot persistence fails.
    pub async fn take_daily_snapshot(
        &self,
        now: DateTime<Utc>,
        event_log_path: Option<&Path>,
    ) -> CoreResult<KpiComputation> {
        let mut computation = self.compute_kpis(now).await?;
        let mut remove_conversion = false;
        if let Some(log_path) = event_log_path {
            let learning = compute_learning_kpis(log_path, now)?;
            computation
                .global
                .metrics
                .insert("corrections_7d".to_string(), learning.corrections_7d);
            computation
                .global
                .metrics
                .insert("stop_blocks_7d".to_string(), learning.stop_blocks_7d);
            computation
                .global
                .metrics
                .insert("recalls_7d".to_string(), learning.recalls_7d);
            if let Some(conversion) = learning.lesson_conversion_7d {
                computation
                    .global
                    .metrics
                    .insert("lesson_conversion_7d".to_string(), conversion);
            } else {
                remove_conversion = true;
            }
        }

        let mut tx = self.pool.begin().await?;
        for scope_snap in std::iter::once(&computation.global).chain(computation.projects.iter()) {
            for (metric, value) in &scope_snap.metrics {
                sqlx::query(
                    r"
                    INSERT INTO kpi_snapshots (day, scope, metric, value)
                    VALUES (?1, ?2, ?3, ?4)
                    ON CONFLICT(day, scope, metric) DO UPDATE SET value = excluded.value
                    ",
                )
                .bind(&computation.day)
                .bind(&scope_snap.scope)
                .bind(metric)
                .bind(value)
                .execute(&mut *tx)
                .await?;
            }
        }
        if remove_conversion {
            sqlx::query(
                "DELETE FROM kpi_snapshots WHERE day = ?1 AND scope = 'global' AND metric = 'lesson_conversion_7d'",
            )
            .bind(&computation.day)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        Ok(computation)
    }

    /// # Errors
    ///
    /// Returns an error if the history query fails.
    pub async fn kpi_history(&self, scope: &str, days: i64) -> CoreResult<KpiHistory> {
        self.kpi_history_at(scope, days, Utc::now()).await
    }

    /// # Errors
    ///
    /// Returns an error if the history query fails.
    pub async fn kpi_history_at(
        &self,
        scope: &str,
        days: i64,
        now: DateTime<Utc>,
    ) -> CoreResult<KpiHistory> {
        let days = days.clamp(1, 3_650);
        let today = now.date_naive();
        let cutoff_day = (today - Duration::days(days - 1))
            .format("%Y-%m-%d")
            .to_string();
        let today_day = today.format("%Y-%m-%d").to_string();

        let rows = sqlx::query_as::<_, SnapshotPointRow>(
            r"
            SELECT day, metric, value
            FROM kpi_snapshots
            WHERE scope = ?1 AND day >= ?2 AND day <= ?3
            ORDER BY day ASC
            ",
        )
        .bind(scope)
        .bind(&cutoff_day)
        .bind(&today_day)
        .fetch_all(&self.pool)
        .await?;

        let mut by_metric: HashMap<String, Vec<KpiPoint>> = HashMap::new();
        for row in rows {
            by_metric.entry(row.metric).or_default().push(KpiPoint {
                day: row.day,
                value: row.value,
            });
        }

        let metrics = KPI_CATALOG
            .iter()
            .filter(|def| metric_applies_to_scope(def, scope))
            .map(|def| KpiSeries {
                name: def.name.to_string(),
                label: def.label.to_string(),
                unit: def.unit.to_string(),
                higher_is_better: def.higher_is_better,
                points: by_metric.remove(def.name).unwrap_or_default(),
            })
            .collect();

        Ok(KpiHistory { metrics })
    }

    /// # Errors
    ///
    /// Returns an error if the baseline query fails.
    pub async fn kpi_baseline_before(
        &self,
        scope: &str,
        target_day: &str,
    ) -> CoreResult<BTreeMap<String, f64>> {
        let rows = sqlx::query_as::<_, BaselineRow>(
            r"
            SELECT metric, value
            FROM kpi_snapshots s1
            WHERE scope = ?1
              AND day = (
                  SELECT MAX(s2.day)
                  FROM kpi_snapshots s2
                  WHERE s2.scope = ?1 AND s2.metric = s1.metric AND s2.day <= ?2
              )
            ",
        )
        .bind(scope)
        .bind(target_day)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.metric, r.value)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateProjectInput, MemoryRelation, RecordActivityInput, SaveMemoryInput};
    use chrono::TimeZone;

    async fn setup() -> Db {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");
        db
    }

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, 12, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    async fn seed_kpi_activity(db: &Db, proj_id: Uuid, global_pref_id: Uuid, now: DateTime<Utc>) {
        for (actor_name, ts) in [
            ("claude", now - Duration::days(1)),
            ("claude", now - Duration::days(3)),
            ("opencode", now - Duration::days(2)),
            ("old-agent", now - Duration::days(15)),
        ] {
            sqlx::query(
                "INSERT INTO activity_events (id, project_id, actor_kind, actor_name, action, entity, entity_id, details, created_at) VALUES (?1, ?2, 'agent', ?3, 'create', 'memory', NULL, NULL, ?4)",
            )
            .bind(Uuid::now_v7())
            .bind(proj_id)
            .bind(actor_name)
            .bind(ts)
            .execute(db.pool())
            .await
            .expect("activity");
        }
        db.record_activity(RecordActivityInput {
            project_id: Some(proj_id),
            actor: crate::models::Actor::User,
            action: ActionKind::Create,
            entity: EntityKind::Memory,
            entity_id: Some(global_pref_id),
            details: None,
        })
        .await
        .expect("user activity");
    }

    async fn seed_kpi_sample(db: &Db, now: DateTime<Utc>) -> Project {
        let proj = db
            .create_project(CreateProjectInput {
                name: "alpha".into(),
                path: "/tmp/alpha".into(),
            })
            .await
            .expect("project");

        let m1 = db
            .save_memory_at(
                SaveMemoryInput {
                    project_id: Some(proj.id),
                    project_name: None,
                    kind: MemoryKind::Decision,
                    title: "Database connection pooling configuration".into(),
                    content: "Use pool".into(),
                    topic_key: Some("db-pool".into()),
                },
                now - Duration::days(2),
                Some(now - Duration::days(2)),
            )
            .await
            .expect("m1");
        let m2 = db
            .save_memory_at(
                SaveMemoryInput {
                    project_id: Some(proj.id),
                    project_name: None,
                    kind: MemoryKind::Pattern,
                    title: "Database connection pooling settings".into(),
                    content: "Pool size 5".into(),
                    topic_key: None,
                },
                now - Duration::days(90),
                Some(now - Duration::days(90)),
            )
            .await
            .expect("m2");
        let archived = db
            .save_memory_at(
                SaveMemoryInput {
                    project_id: Some(proj.id),
                    project_name: None,
                    kind: MemoryKind::Note,
                    title: "Obsolete pooling note".into(),
                    content: "Old".into(),
                    topic_key: Some("old-pool".into()),
                },
                now - Duration::days(100),
                Some(now - Duration::days(100)),
            )
            .await
            .expect("archived");
        let global_pref = db
            .save_memory_at(
                SaveMemoryInput {
                    project_id: None,
                    project_name: None,
                    kind: MemoryKind::Preference,
                    title: "Always use biome".into(),
                    content: "What: Biome always".into(),
                    topic_key: Some("biome-rule".into()),
                },
                now - Duration::days(1),
                Some(now - Duration::days(1)),
            )
            .await
            .expect("pref");

        sqlx::query("UPDATE memories SET superseded_by = ?1 WHERE id = ?2")
            .bind(m1.id)
            .bind(archived.id)
            .execute(db.pool())
            .await
            .expect("archive");

        sqlx::query("UPDATE memories SET last_used_at = ?1, use_count = 1 WHERE id = ?2")
            .bind(now - Duration::days(5))
            .bind(m1.id)
            .execute(db.pool())
            .await
            .expect("touch m1");

        db.link_memories(m1.id, m2.id, MemoryRelation::Related)
            .await
            .expect("link active");
        db.link_memories(m1.id, archived.id, MemoryRelation::Related)
            .await
            .expect("link archived");

        seed_kpi_activity(db, proj.id, global_pref.id, now).await;
        proj
    }

    #[tokio::test]
    async fn compute_kpis_handles_active_archived_links_dupes_stale_and_index_bytes() {
        let db = setup().await;
        let now = fixed_now();
        let proj = seed_kpi_sample(&db, now).await;

        let kpis = db.compute_kpis(now).await.expect("compute_kpis");
        assert_eq!(kpis.day, "2026-11-26");

        let g = &kpis.global.metrics;
        assert_eq!(g["memories_active"], 3.0);
        assert_eq!(g["memories_archived"], 1.0);
        assert_eq!(g["preferences_active"], 1.0);
        assert_eq!(g["duplicate_pairs"], 1.0);
        assert_eq!(g["stale_memories"], 1.0);
        assert!((g["link_density"] - (1.0 / 3.0)).abs() < 1e-9);
        assert!((g["topic_key_ratio"] - (2.0 / 3.0)).abs() < 1e-9);
        assert!((g["reuse_rate_30d"] - (1.0 / 3.0)).abs() < 1e-9);
        assert_eq!(g["saves_7d"], 3.0);
        assert_eq!(g["agents_active_7d"], 2.0);
        assert!(!g.contains_key("index_bytes"));

        assert_eq!(kpis.projects.len(), 1);
        let p = &kpis.projects[0];
        assert_eq!(p.scope, proj.id.to_string());
        assert_eq!(p.metrics["memories_active"], 2.0);
        assert_eq!(p.metrics["memories_archived"], 1.0);
        assert_eq!(p.metrics["preferences_active"], 0.0);
        assert_eq!(p.metrics["duplicate_pairs"], 1.0);
        assert_eq!(p.metrics["stale_memories"], 1.0);
        assert!((p.metrics["link_density"] - 0.5).abs() < 1e-9);
        assert!((p.metrics["topic_key_ratio"] - 0.5).abs() < 1e-9);
        assert!((p.metrics["reuse_rate_30d"] - 0.5).abs() < 1e-9);
        assert_eq!(p.metrics["saves_7d"], 3.0);
        assert_eq!(p.metrics["agents_active_7d"], 2.0);
        assert!(p.metrics["index_bytes"] > 50.0);
    }

    #[tokio::test]
    async fn snapshot_is_idempotent_and_history_returns_series() {
        let db = setup().await;
        let now = fixed_now();
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("kpi-events.jsonl");

        let events = [
            serde_json::json!({
                "ts": (now - Duration::days(2)).to_rfc3339(),
                "event": "correction",
                "session_id": "s1"
            }),
            serde_json::json!({
                "ts": (now - Duration::days(2)).to_rfc3339(),
                "event": "stop",
                "session_id": "s1",
                "corrections": 1,
                "failed_tools": 0,
                "saved": false,
                "blocked": true
            }),
            serde_json::json!({
                "ts": (now - Duration::days(1)).to_rfc3339(),
                "event": "stop",
                "session_id": "s1",
                "corrections": 1,
                "failed_tools": 0,
                "saved": true,
                "blocked": false
            }),
            serde_json::json!({
                "ts": (now - Duration::days(1)).to_rfc3339(),
                "event": "stop",
                "session_id": "s2",
                "corrections": 2,
                "failed_tools": 1,
                "saved": false,
                "blocked": true
            }),
        ];
        let body = events
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&log_path, body).expect("write log");

        db.take_daily_snapshot(now - Duration::days(1), Some(&log_path))
            .await
            .expect("snap day-1");
        db.save_memory_at(
            SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Note,
                title: "Today note".into(),
                content: "Content".into(),
                topic_key: None,
            },
            now,
            Some(now),
        )
        .await
        .expect("save today");
        db.take_daily_snapshot(now, Some(&log_path))
            .await
            .expect("snap today 1");
        db.save_memory_at(
            SaveMemoryInput {
                project_id: None,
                project_name: None,
                kind: MemoryKind::Note,
                title: "Second note today".into(),
                content: "Content 2".into(),
                topic_key: None,
            },
            now,
            Some(now),
        )
        .await
        .expect("save today 2");
        db.take_daily_snapshot(now, Some(&log_path))
            .await
            .expect("snap today 2");

        let history = db.kpi_history_at("global", 7, now).await.expect("history");
        let active_series = history
            .metrics
            .iter()
            .find(|m| m.name == "memories_active")
            .expect("active series");
        assert_eq!(active_series.points.len(), 2);
        assert_eq!(active_series.points[0].day, "2026-11-25");
        assert_eq!(active_series.points[0].value, 0.0);
        assert_eq!(active_series.points[1].day, "2026-11-26");
        assert_eq!(active_series.points[1].value, 2.0);

        let conv = history
            .metrics
            .iter()
            .find(|m| m.name == "lesson_conversion_7d")
            .expect("conversion series");
        assert_eq!(conv.points.len(), 2);
        assert!((conv.points[1].value - 0.5).abs() < 1e-9);
    }

    #[test]
    fn learning_kpis_handles_missing_file_rotation_and_empty_corrections() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("kpi-events.jsonl");
        let now = fixed_now();

        let missing = compute_learning_kpis(&log_path, now).expect("missing file ok");
        assert_eq!(missing.corrections_7d, 0.0);
        assert_eq!(missing.stop_blocks_7d, 0.0);
        assert_eq!(missing.lesson_conversion_7d, None);

        let rotated_a = dir.path().join("kpi-events.1700000001000.11.jsonl");
        let rotated_b = dir.path().join("kpi-events.1700000002000.12.jsonl");
        std::fs::write(
            &rotated_a,
            format!(
                "{}\n{}\n",
                serde_json::json!({"ts": (now - Duration::days(3)).to_rfc3339(), "event": "correction"}),
                serde_json::json!({"ts": (now - Duration::days(10)).to_rfc3339(), "event": "correction"}),
            ),
        )
        .expect("write rotated a");
        std::fs::write(
            &rotated_b,
            format!(
                "{}\n{}\n{}\n",
                serde_json::json!({"ts": (now - Duration::days(1)).to_rfc3339(), "event": "correction"}),
                serde_json::json!({"ts": (now - Duration::days(2)).to_rfc3339(), "event": "recall"}),
                serde_json::json!({"ts": (now - Duration::days(9)).to_rfc3339(), "event": "recall"}),
            ),
        )
        .expect("write rotated b");
        std::fs::write(
            &log_path,
            format!(
                "{}\nnot-json\n",
                serde_json::json!({
                    "ts": (now - Duration::hours(2)).to_rfc3339(),
                    "event": "stop",
                    "session_id": "s-only-tools",
                    "corrections": 0,
                    "failed_tools": 4,
                    "saved": false,
                    "blocked": true
                }),
            ),
        )
        .expect("write current");

        let stats = compute_learning_kpis(&log_path, now).expect("stats");
        assert_eq!(stats.corrections_7d, 2.0);
        assert_eq!(stats.stop_blocks_7d, 1.0);
        assert_eq!(stats.recalls_7d, 1.0, "only the recall inside 7 days");
        assert_eq!(stats.lesson_conversion_7d, None);
    }

    async fn save_test_note(db: &Db, project_id: Uuid, title: &str, now: DateTime<Utc>) {
        db.save_memory_at(
            SaveMemoryInput {
                project_id: Some(project_id),
                project_name: None,
                kind: MemoryKind::Note,
                title: title.into(),
                content: "Content".into(),
                topic_key: None,
            },
            now,
            Some(now),
        )
        .await
        .expect("save note");
    }

    #[tokio::test]
    async fn duplicate_project_names_and_project_named_global_keep_distinct_uuid_scopes() {
        let db = setup().await;
        let now = fixed_now();
        let p1 = db
            .create_project(CreateProjectInput {
                name: "shared".into(),
                path: "/tmp/shared-1".into(),
            })
            .await
            .expect("p1");
        let p2 = db
            .create_project(CreateProjectInput {
                name: "shared".into(),
                path: "/tmp/shared-2".into(),
            })
            .await
            .expect("p2");
        let p_global = db
            .create_project(CreateProjectInput {
                name: "global".into(),
                path: "/tmp/global-proj".into(),
            })
            .await
            .expect("p_global");

        save_test_note(&db, p1.id, "Memory in p1", now).await;
        save_test_note(&db, p2.id, "Memory in p2 a", now).await;
        save_test_note(&db, p2.id, "Memory in p2 b", now).await;
        save_test_note(&db, p_global.id, "Memory in project named global", now).await;

        db.take_daily_snapshot(now, None).await.expect("snapshot");

        let global_history = db.kpi_history_at("global", 7, now).await.expect("h_global");
        let first_proj_history = db
            .kpi_history_at(&p1.id.to_string(), 7, now)
            .await
            .expect("h_p1");
        let second_proj_history = db
            .kpi_history_at(&p2.id.to_string(), 7, now)
            .await
            .expect("h_p2");
        let named_global_history = db
            .kpi_history_at(&p_global.id.to_string(), 7, now)
            .await
            .expect("h_pg");

        let val = |h: &KpiHistory, name: &str| {
            h.metrics
                .iter()
                .find(|m| m.name == name)
                .and_then(|m| m.points.last())
                .map(|p| p.value)
        };
        assert_eq!(val(&global_history, "memories_active"), Some(4.0));
        assert_eq!(val(&first_proj_history, "memories_active"), Some(1.0));
        assert_eq!(val(&second_proj_history, "memories_active"), Some(2.0));
        assert_eq!(val(&named_global_history, "memories_active"), Some(1.0));
        assert!(val(&named_global_history, "index_bytes").is_some());
    }

    #[tokio::test]
    async fn kpi_scope_migration_maps_unique_names_and_drops_ambiguous_or_missing() {
        let db = Db::connect_in_memory().await.expect("db");
        let mig_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<_> = std::fs::read_dir(&mig_dir)
            .expect("read migrations")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("sql"))
            .collect();
        files.sort();
        for path in &files {
            if path.file_name().and_then(|n| n.to_str())
                == Some("20261202000000_kpi_scope_project_id.sql")
            {
                break;
            }
            let sql = std::fs::read_to_string(path).expect("read mig");
            sqlx::raw_sql(&sql).execute(db.pool()).await.expect("apply");
        }

        let unique = db
            .create_project(CreateProjectInput {
                name: "unique-proj".into(),
                path: "/tmp/unique-proj".into(),
            })
            .await
            .expect("unique");
        db.create_project(CreateProjectInput {
            name: "dup-proj".into(),
            path: "/tmp/dup-proj-1".into(),
        })
        .await
        .expect("dup1");
        db.create_project(CreateProjectInput {
            name: "dup-proj".into(),
            path: "/tmp/dup-proj-2".into(),
        })
        .await
        .expect("dup2");

        for (scope, value) in [
            ("global", 10.0),
            ("unique-proj", 20.0),
            ("dup-proj", 30.0),
            ("deleted-proj", 40.0),
        ] {
            sqlx::query(
                "INSERT INTO kpi_snapshots (day, scope, metric, value) VALUES ('2026-11-25', ?1, 'memories_active', ?2)",
            )
            .bind(scope)
            .bind(value)
            .execute(db.pool())
            .await
            .expect("insert snapshot");
        }

        let migration_sql =
            std::fs::read_to_string(mig_dir.join("20261202000000_kpi_scope_project_id.sql"))
                .expect("read new mig");
        sqlx::raw_sql(&migration_sql)
            .execute(db.pool())
            .await
            .expect("apply new mig");

        let rows: Vec<(String, f64)> = sqlx::query_as(
            "SELECT scope, value FROM kpi_snapshots WHERE metric = 'memories_active' ORDER BY value ASC",
        )
        .fetch_all(db.pool())
        .await
        .expect("select migrated");
        assert_eq!(
            rows,
            vec![("global".to_string(), 10.0), (unique.id.to_string(), 20.0),]
        );
    }

    #[tokio::test]
    async fn snapshot_with_200_memories_runs_quickly() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("saispace.db"))
            .await
            .expect("open");
        let now = fixed_now();
        let mut projects = Vec::new();
        for idx in 0..4 {
            projects.push(
                db.create_project(CreateProjectInput {
                    name: format!("proj-{idx}"),
                    path: format!("/tmp/proj-{idx}"),
                })
                .await
                .expect("project"),
            );
        }
        let mut tx = db.pool().begin().await.expect("tx");
        for i in 0_i64..200 {
            let proj_idx = usize::try_from(i % 4).expect("project index");
            let project_id = if i % 5 == 0 {
                None
            } else {
                Some(projects[proj_idx].id)
            };
            let project_name = if i % 5 == 0 {
                String::new()
            } else {
                projects[proj_idx].name.clone()
            };
            sqlx::query(
                r"
                INSERT INTO memories (id, project_id, project_name, kind, title, content, topic_key, created_at, updated_at)
                VALUES (?1, ?2, ?3, 'decision', ?4, 'Contenido de prueba con detalles técnicos', '', ?5, ?5)
                ",
            )
            .bind(Uuid::now_v7())
            .bind(project_id)
            .bind(project_name)
            .bind(format!("Decision arquitectura modulo {i} servicio alpha"))
            .bind(now - Duration::days(i % 90))
            .execute(&mut *tx)
            .await
            .expect("insert memory");
        }
        tx.commit().await.expect("commit");

        let log_path = dir.path().join("kpi-events.jsonl");
        let t0 = std::time::Instant::now();
        db.take_daily_snapshot(now, Some(&log_path))
            .await
            .expect("snapshot");
        let elapsed = t0.elapsed();
        eprintln!("take_daily_snapshot (200 memories, 4 projects): {elapsed:?}");
        // The snapshot runs detached from the session hook, so this only
        // guards against pathological scaling: a tight bound flakes on busy
        // CI runners and debug builds (~60 ms there for this data).
        assert!(elapsed.as_millis() < 1_000, "took {elapsed:?}");
    }
}
