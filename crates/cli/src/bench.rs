use crate::{CliError, CliResult};
use chrono::Utc;
use clap::Args;
use saispace_core::Db;
use saispace_core::learning::{analyze_transcript, is_correction};
use saispace_core::memory_render::render_context;
use saispace_core::models::{
    CreateProjectInput, MemoryKind, MemoryScope, SaveMemoryInput, SearchKind,
};
use saispace_rules::find_root;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use uuid::Uuid;

const BENCH_VERSION: u32 = 1;
const CONTEXT_LIMIT: i64 = 12;
const SCALE_MEMORIES: usize = 1_000;
const SCALE_CONTEXT_ITERS: usize = 7;
const SCALE_SAVE_ITERS: usize = 15;
const RATIO_REGRESSION_THRESHOLD: f64 = 0.01;
const SIZE_REGRESSION_PCT: f64 = 0.03;
const EPSILON: f64 = 1e-9;

#[derive(Args)]
pub struct BenchArgs {
    #[arg(long, help = "Muestra los resultados en formato JSON")]
    pub json: bool,
    #[arg(long, help = "Guarda la medición en bench/history.jsonl")]
    pub record: bool,
    #[arg(
        long,
        help = "Compara contra la última medición registrada en bench/history.jsonl"
    )]
    pub compare: bool,
    #[arg(long, value_name = "DIR", help = "Directorio de fixtures a usar")]
    pub fixtures: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Metric {
    pub value: f64,
    pub unit: String,
    pub higher_is_better: bool,
    pub informational: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    pub version: u32,
    pub git_sha: String,
    pub date: String,
    pub metrics: BTreeMap<String, Metric>,
}

#[derive(Deserialize)]
struct CorrectionFixture {
    prompt: String,
    correction: bool,
}

#[derive(Deserialize)]
struct MemoryFixture {
    key: String,
    kind: String,
    title: String,
    content: String,
    #[serde(default)]
    topic_key: Option<String>,
}

#[derive(Deserialize)]
struct QueryFixture {
    query: String,
    expected: Vec<String>,
}

#[derive(Deserialize)]
struct DupeFixture {
    a: String,
    b: String,
    duplicate: bool,
}

struct Fixtures {
    corrections: Vec<CorrectionFixture>,
    transcripts: Vec<(String, String, bool)>,
    memories: Vec<MemoryFixture>,
    queries: Vec<QueryFixture>,
    dupes: Vec<DupeFixture>,
    /// Blind set in `bench/holdout/`, written by a different model than the
    /// one tuning the detector: it catches rules that memorize the fixtures.
    holdout_corrections: Vec<CorrectionFixture>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricTrend {
    Equal,
    Better,
    Worse,
    Regressed,
}

struct ComparisonRow {
    label: String,
    current: Metric,
    previous: Option<Metric>,
    trend: MetricTrend,
}

const METRIC_LABELS: &[(&str, &str)] = &[
    ("correction_precision", "Precisión correcciones"),
    ("correction_recall", "Cobertura correcciones"),
    ("correction_f1", "F1 correcciones"),
    (
        "holdout_correction_precision",
        "Precisión correcciones (ciego)",
    ),
    (
        "holdout_correction_recall",
        "Cobertura correcciones (ciego)",
    ),
    ("holdout_correction_f1", "F1 correcciones (ciego)"),
    ("stop_accuracy", "Exactitud hook Stop"),
    ("search_recall_at_5", "Recall@5 búsqueda"),
    ("search_mrr", "MRR búsqueda"),
    ("dupes_precision", "Precisión duplicados"),
    ("dupes_recall", "Cobertura duplicados"),
    ("index_bytes", "Tamaño índice"),
    ("index_tokens_est", "Tokens índice (est.)"),
    ("latency_context_ms_p50", "Latencia contexto p50 (info)"),
    ("latency_search_ms_p50", "Latencia búsqueda p50 (info)"),
    ("latency_save_ms_p50", "Latencia guardado p50 (info)"),
];

fn count_to_f64(count: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let value = count as f64;
    value
}

fn ratio_metric(value: f64) -> Metric {
    Metric {
        value,
        unit: "ratio".to_string(),
        higher_is_better: true,
        informational: false,
    }
}

fn size_metric(value: usize, unit: &str) -> Metric {
    Metric {
        value: count_to_f64(value),
        unit: unit.to_string(),
        higher_is_better: false,
        informational: false,
    }
}

fn latency_metric(value: f64) -> Metric {
    Metric {
        value,
        unit: "ms".to_string(),
        higher_is_better: false,
        informational: true,
    }
}

fn precision_recall_f1(tp: usize, fp: usize, fn_count: usize) -> (f64, f64, f64) {
    let precision = if tp + fp == 0 {
        0.0
    } else {
        count_to_f64(tp) / count_to_f64(tp + fp)
    };
    let recall = if tp + fn_count == 0 {
        0.0
    } else {
        count_to_f64(tp) / count_to_f64(tp + fn_count)
    };
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    (precision, recall, f1)
}

fn accuracy(correct: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count_to_f64(correct) / count_to_f64(total)
    }
}

fn query_recall_at_k(hits: &[String], expected: &[String], k: usize) -> f64 {
    if expected.is_empty() {
        return 0.0;
    }
    let top_k: HashSet<&str> = hits.iter().take(k).map(String::as_str).collect();
    let expected_set: HashSet<&str> = expected.iter().map(String::as_str).collect();
    let matched = expected_set
        .iter()
        .filter(|key| top_k.contains(**key))
        .count();
    count_to_f64(matched) / count_to_f64(expected_set.len())
}

fn query_reciprocal_rank(hits: &[String], expected: &[String]) -> f64 {
    if expected.is_empty() {
        return 0.0;
    }
    let expected_set: HashSet<&str> = expected.iter().map(String::as_str).collect();
    hits.iter()
        .position(|hit| expected_set.contains(hit.as_str()))
        .map_or(0.0, |idx| 1.0 / count_to_f64(idx + 1))
}

fn search_metrics(results: &[(&[String], &[String])]) -> (f64, f64) {
    if results.is_empty() {
        return (0.0, 0.0);
    }
    let mut recall_sum = 0.0;
    let mut mrr_sum = 0.0;
    for (hits, expected) in results {
        recall_sum += query_recall_at_k(hits, expected, 5);
        mrr_sum += query_reciprocal_rank(hits, expected);
    }
    let denom = count_to_f64(results.len());
    (recall_sum / denom, mrr_sum / denom)
}

fn median_ms(samples: &mut [f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(f64::total_cmp);
    let mid = samples.len() / 2;
    if samples.len() % 2 == 1 {
        samples[mid]
    } else {
        f64::midpoint(samples[mid - 1], samples[mid])
    }
}

fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn parse_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> CliResult<Vec<T>> {
    let content = fs::read_to_string(path)?;
    let mut items = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item = serde_json::from_str::<T>(trimmed).map_err(|err| {
            CliError::Usage(format!(
                "JSON inválido en {}:{}: {err}",
                path.display(),
                idx + 1
            ))
        })?;
        items.push(item);
    }
    Ok(items)
}

fn load_fixtures(dir: &Path) -> CliResult<Fixtures> {
    if !dir.is_dir() {
        return Err(CliError::Usage(format!(
            "no existe el directorio de fixtures: {}",
            dir.display()
        )));
    }
    let corrections: Vec<CorrectionFixture> = parse_jsonl(&dir.join("corrections.jsonl"))?;
    let memories: Vec<MemoryFixture> = parse_jsonl(&dir.join("memories.jsonl"))?;
    let queries: Vec<QueryFixture> = parse_jsonl(&dir.join("queries.jsonl"))?;
    let dupes: Vec<DupeFixture> = parse_jsonl(&dir.join("dupes.jsonl"))?;

    let expected_path = dir.join("transcripts/expected.json");
    let expected_map: BTreeMap<String, bool> =
        serde_json::from_str(&fs::read_to_string(&expected_path)?)?;
    let mut transcripts = Vec::with_capacity(expected_map.len());
    for (name, expected_block) in expected_map {
        let transcript_path = dir.join("transcripts").join(&name);
        let body = fs::read_to_string(&transcript_path)?;
        transcripts.push((name, body, expected_block));
    }

    validate_fixtures(&corrections, &transcripts, &memories, &queries, &dupes)?;
    // Every `corrections*.jsonl` in the holdout dir: the blind set plus
    // realistic developer prompts that must not fire.
    let mut holdout_corrections = Vec::new();
    if let Ok(entries) = fs::read_dir(dir.join("../holdout")) {
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.starts_with("corrections")
                        && std::path::Path::new(n)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
                })
            })
            .collect();
        paths.sort();
        for path in paths {
            holdout_corrections.extend(parse_jsonl::<CorrectionFixture>(&path)?);
        }
    }
    Ok(Fixtures {
        corrections,
        transcripts,
        memories,
        queries,
        dupes,
        holdout_corrections,
    })
}

fn validate_fixtures(
    corrections: &[CorrectionFixture],
    transcripts: &[(String, String, bool)],
    memories: &[MemoryFixture],
    queries: &[QueryFixture],
    dupes: &[DupeFixture],
) -> CliResult<()> {
    if corrections.is_empty()
        || transcripts.is_empty()
        || memories.is_empty()
        || queries.is_empty()
        || dupes.is_empty()
    {
        return Err(CliError::Usage(
            "todos los ficheros de fixtures deben tener al menos un registro".to_string(),
        ));
    }
    let mut keys = HashSet::with_capacity(memories.len());
    for m in memories {
        if !keys.insert(m.key.as_str()) {
            return Err(CliError::Usage(format!(
                "clave duplicada en memories.jsonl: {}",
                m.key
            )));
        }
        if MemoryKind::parse(&m.kind).is_none() {
            return Err(CliError::Usage(format!(
                "kind desconocido '{}' en memories.jsonl ({})",
                m.kind, m.key
            )));
        }
    }
    for q in queries {
        if q.expected.is_empty() {
            return Err(CliError::Usage(format!(
                "la consulta '{}' debe tener al menos una clave esperada",
                q.query
            )));
        }
        for key in &q.expected {
            if !keys.contains(key.as_str()) {
                return Err(CliError::Usage(format!(
                    "consulta '{}' referencia clave inexistente '{key}'",
                    q.query
                )));
            }
        }
    }
    for d in dupes {
        if !keys.contains(d.a.as_str()) || !keys.contains(d.b.as_str()) {
            return Err(CliError::Usage(format!(
                "par duplicado ({}, {}) referencia clave inexistente",
                d.a, d.b
            )));
        }
    }
    Ok(())
}

fn score_corrections(items: &[CorrectionFixture]) -> (f64, f64, f64) {
    let mut tp = 0;
    let mut fp = 0;
    let mut fn_count = 0;
    for item in items {
        let predicted = is_correction(&item.prompt);
        match (predicted, item.correction) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => fn_count += 1,
            (false, false) => {}
        }
    }
    precision_recall_f1(tp, fp, fn_count)
}

fn score_stop_hook(transcripts: &[(String, String, bool)]) -> f64 {
    let mut correct = 0;
    for (_, jsonl, expected_block) in transcripts {
        let signals = analyze_transcript(jsonl);
        let predicted_block = !signals.saved_after_last_correction
            && (signals.corrections > 0 || signals.failed_tools >= 3);
        if predicted_block == *expected_block {
            correct += 1;
        }
    }
    accuracy(correct, transcripts.len())
}

async fn setup_bench_db(project_name: &str, project_path: &str) -> CliResult<(Db, Uuid)> {
    let db = Db::connect_in_memory().await?;
    db.migrate().await?;
    let project = db
        .create_project(CreateProjectInput {
            name: project_name.to_string(),
            path: project_path.to_string(),
        })
        .await?;
    Ok((db, project.id))
}

async fn seed_memories(
    db: &Db,
    project_id: Uuid,
    memories: &[MemoryFixture],
) -> CliResult<HashMap<Uuid, String>> {
    let mut id_to_key = HashMap::with_capacity(memories.len());
    for item in memories {
        let kind = MemoryKind::parse(&item.kind).ok_or_else(|| {
            CliError::Usage(format!(
                "kind desconocido '{}' en memories.jsonl",
                item.kind
            ))
        })?;
        let saved = db
            .save_memory(SaveMemoryInput {
                project_id: Some(project_id),
                project_name: None,
                kind,
                title: item.title.clone(),
                content: item.content.clone(),
                topic_key: item.topic_key.clone(),
            })
            .await?;
        id_to_key.insert(saved.id, item.key.clone());
    }
    Ok(id_to_key)
}

async fn score_search(
    db: &Db,
    project_id: Uuid,
    queries: &[QueryFixture],
    id_to_key: &HashMap<Uuid, String>,
) -> CliResult<(f64, f64)> {
    let mut resolved: Vec<(Vec<String>, Vec<String>)> = Vec::with_capacity(queries.len());
    for q in queries {
        let hits = match db.search(&q.query, Some(project_id), None).await {
            Ok(hits) => hits,
            Err(saispace_core::CoreError::Validation(_)) => Vec::new(),
            Err(err) => return Err(err.into()),
        };
        let hit_keys: Vec<String> = hits
            .into_iter()
            .filter(|h| h.kind == SearchKind::Memory)
            .filter_map(|h| id_to_key.get(&h.id).cloned())
            .collect();
        resolved.push((hit_keys, q.expected.clone()));
    }
    let refs: Vec<(&[String], &[String])> = resolved
        .iter()
        .map(|(h, e)| (h.as_slice(), e.as_slice()))
        .collect();
    Ok(search_metrics(&refs))
}

async fn score_dupes(
    db: &Db,
    project_id: Uuid,
    dupes: &[DupeFixture],
    id_to_key: &HashMap<Uuid, String>,
) -> CliResult<(f64, f64)> {
    let pairs = db
        .duplicate_pairs(MemoryScope::Project(project_id), 500)
        .await?;
    let mut predicted = HashSet::with_capacity(pairs.len());
    for pair in pairs {
        if let (Some(ka), Some(kb)) = (id_to_key.get(&pair.a.id), id_to_key.get(&pair.b.id)) {
            predicted.insert(canonical_pair(ka, kb));
        }
    }
    let mut tp = 0;
    let mut fp = 0;
    let mut fn_count = 0;
    for item in dupes {
        let key = canonical_pair(&item.a, &item.b);
        match (predicted.contains(&key), item.duplicate) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => fn_count += 1,
            (false, false) => {}
        }
    }
    let (precision, recall, _) = precision_recall_f1(tp, fp, fn_count);
    Ok((precision, recall))
}

async fn measure_index_size(db: &Db, project_id: Uuid) -> CliResult<(usize, usize)> {
    let project = db.get_project(project_id).await?;
    let context = db
        .memory_context(MemoryScope::Project(project_id), CONTEXT_LIMIT)
        .await?;
    let rendered = render_context(project.as_ref(), &context);
    let bytes = rendered.len();
    Ok((bytes, bytes.div_ceil(4)))
}

async fn measure_scaled_latencies(fixtures: &Fixtures) -> CliResult<(f64, f64, f64)> {
    let (db, project_id) = setup_bench_db("novapay-api-scale", "/bench/novapay-api-scale").await?;
    for i in 0..SCALE_MEMORIES {
        let base = &fixtures.memories[i % fixtures.memories.len()];
        let batch = i / fixtures.memories.len();
        let kind = MemoryKind::parse(&base.kind).ok_or_else(|| {
            CliError::Usage(format!(
                "kind desconocido '{}' en memories.jsonl",
                base.kind
            ))
        })?;
        let (title, content, topic_key) = if batch == 0 {
            (
                base.title.clone(),
                base.content.clone(),
                base.topic_key.clone(),
            )
        } else {
            (
                format!("{} [{batch:02}]", base.title),
                format!("{}\nScale ref: {i:04}", base.content),
                base.topic_key.as_ref().map(|k| format!("{k}-s{batch:02}")),
            )
        };
        db.save_memory(SaveMemoryInput {
            project_id: Some(project_id),
            project_name: None,
            kind,
            title,
            content,
            topic_key,
        })
        .await?;
    }

    let project = db.get_project(project_id).await?;
    for _ in 0..1 {
        let ctx = db
            .memory_context(MemoryScope::Project(project_id), CONTEXT_LIMIT)
            .await?;
        let _ = render_context(project.as_ref(), &ctx);
    }
    let mut context_samples = Vec::with_capacity(SCALE_CONTEXT_ITERS);
    for _ in 0..SCALE_CONTEXT_ITERS {
        let start = Instant::now();
        let ctx = db
            .memory_context(MemoryScope::Project(project_id), CONTEXT_LIMIT)
            .await?;
        let _ = render_context(project.as_ref(), &ctx);
        context_samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    for q in fixtures.queries.iter().take(2) {
        let _ = db.search(&q.query, Some(project_id), None).await;
    }
    let mut search_samples = Vec::with_capacity(fixtures.queries.len() * 2);
    for _ in 0..2 {
        for q in &fixtures.queries {
            let start = Instant::now();
            let _ = db.search(&q.query, Some(project_id), None).await;
            search_samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    let mut save_samples = Vec::with_capacity(SCALE_SAVE_ITERS);
    for i in 0..SCALE_SAVE_ITERS {
        let base = &fixtures.memories[i % fixtures.memories.len()];
        let kind = MemoryKind::parse(&base.kind).ok_or_else(|| {
            CliError::Usage(format!(
                "kind desconocido '{}' en memories.jsonl",
                base.kind
            ))
        })?;
        let start = Instant::now();
        db.save_memory(SaveMemoryInput {
            project_id: Some(project_id),
            project_name: None,
            kind,
            title: format!("{} [probe-{i:02}]", base.title),
            content: format!("{}\nProbe ref: {i:04}", base.content),
            topic_key: Some(format!("bench-probe-{i:02}")),
        })
        .await?;
        save_samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    Ok((
        median_ms(&mut context_samples),
        median_ms(&mut search_samples),
        median_ms(&mut save_samples),
    ))
}

async fn evaluate_all(fixtures: &Fixtures, root: &Path) -> CliResult<BenchReport> {
    let (corr_p, corr_r, corr_f1) = score_corrections(&fixtures.corrections);
    let stop_acc = score_stop_hook(&fixtures.transcripts);

    let (db, project_id) = setup_bench_db("novapay-api", "/bench/novapay-api").await?;
    let id_to_key = seed_memories(&db, project_id, &fixtures.memories).await?;
    let (search_r5, search_mrr) =
        score_search(&db, project_id, &fixtures.queries, &id_to_key).await?;
    let (dupes_p, dupes_r) = score_dupes(&db, project_id, &fixtures.dupes, &id_to_key).await?;
    let (index_bytes, index_tokens_est) = measure_index_size(&db, project_id).await?;
    let (ctx_ms, search_ms, save_ms) = measure_scaled_latencies(fixtures).await?;

    let mut metrics = BTreeMap::new();
    metrics.insert("correction_precision".to_string(), ratio_metric(corr_p));
    metrics.insert("correction_recall".to_string(), ratio_metric(corr_r));
    metrics.insert("correction_f1".to_string(), ratio_metric(corr_f1));
    if !fixtures.holdout_corrections.is_empty() {
        let (p, r, f1) = score_corrections(&fixtures.holdout_corrections);
        metrics.insert("holdout_correction_precision".to_string(), ratio_metric(p));
        metrics.insert("holdout_correction_recall".to_string(), ratio_metric(r));
        metrics.insert("holdout_correction_f1".to_string(), ratio_metric(f1));
    }
    metrics.insert("stop_accuracy".to_string(), ratio_metric(stop_acc));
    metrics.insert("search_recall_at_5".to_string(), ratio_metric(search_r5));
    metrics.insert("search_mrr".to_string(), ratio_metric(search_mrr));
    metrics.insert("dupes_precision".to_string(), ratio_metric(dupes_p));
    metrics.insert("dupes_recall".to_string(), ratio_metric(dupes_r));
    metrics.insert("index_bytes".to_string(), size_metric(index_bytes, "bytes"));
    metrics.insert(
        "index_tokens_est".to_string(),
        size_metric(index_tokens_est, "tokens"),
    );
    metrics.insert("latency_context_ms_p50".to_string(), latency_metric(ctx_ms));
    metrics.insert(
        "latency_search_ms_p50".to_string(),
        latency_metric(search_ms),
    );
    metrics.insert("latency_save_ms_p50".to_string(), latency_metric(save_ms));

    Ok(BenchReport {
        version: BENCH_VERSION,
        git_sha: git_sha(root),
        date: Utc::now().to_rfc3339(),
        metrics,
    })
}

fn git_sha(root: &Path) -> String {
    let Ok(out) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
    else {
        return "unknown".to_string();
    };
    if !out.status.success() {
        return "unknown".to_string();
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return "unknown".to_string();
    }
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .is_ok_and(|s| s.status.success() && !String::from_utf8_lossy(&s.stdout).trim().is_empty());
    if dirty { format!("{sha}-dirty") } else { sha }
}

fn read_last_history(history_path: &Path) -> CliResult<Option<BenchReport>> {
    if !history_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(history_path)?;
    let Some(last) = content.lines().rev().find(|l| !l.trim().is_empty()) else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_str::<BenchReport>(last)?))
}

fn append_history(history_path: &Path, report: &BenchReport) -> CliResult<()> {
    if let Some(parent) = history_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path)?;
    writeln!(file, "{}", serde_json::to_string(report)?)?;
    Ok(())
}

fn classify_change(prev: &Metric, curr: &Metric) -> MetricTrend {
    let delta = curr.value - prev.value;
    if delta.abs() <= EPSILON {
        return MetricTrend::Equal;
    }
    let regressed = if curr.informational {
        false
    } else {
        match curr.unit.as_str() {
            "ratio" => {
                if curr.higher_is_better {
                    prev.value - curr.value > RATIO_REGRESSION_THRESHOLD + EPSILON
                } else {
                    curr.value - prev.value > RATIO_REGRESSION_THRESHOLD + EPSILON
                }
            }
            "bytes" | "tokens" => {
                if curr.higher_is_better {
                    prev.value > 0.0
                        && (prev.value - curr.value) / prev.value > SIZE_REGRESSION_PCT + EPSILON
                } else if prev.value <= 0.0 {
                    curr.value > EPSILON
                } else {
                    (curr.value - prev.value) / prev.value > SIZE_REGRESSION_PCT + EPSILON
                }
            }
            _ => false,
        }
    };
    if regressed {
        return MetricTrend::Regressed;
    }
    let improved = if curr.higher_is_better {
        delta > EPSILON
    } else {
        delta < -EPSILON
    };
    if improved {
        MetricTrend::Better
    } else {
        MetricTrend::Worse
    }
}

fn build_comparison(current: &BenchReport, previous: &BenchReport) -> (Vec<ComparisonRow>, bool) {
    let mut rows = Vec::new();
    let mut has_regression = false;
    let mut seen = HashSet::new();

    for &(key, label) in METRIC_LABELS {
        if let Some(metric) = current.metrics.get(key) {
            seen.insert(key);
            let prev = previous.metrics.get(key).cloned();
            let trend = prev
                .as_ref()
                .map_or(MetricTrend::Equal, |p| classify_change(p, metric));
            if trend == MetricTrend::Regressed {
                has_regression = true;
            }
            rows.push(ComparisonRow {
                label: label.to_string(),
                current: metric.clone(),
                previous: prev,
                trend,
            });
        }
    }

    for (key, metric) in &current.metrics {
        if seen.contains(key.as_str()) {
            continue;
        }
        let prev = previous.metrics.get(key).cloned();
        let trend = prev
            .as_ref()
            .map_or(MetricTrend::Equal, |p| classify_change(p, metric));
        if trend == MetricTrend::Regressed {
            has_regression = true;
        }
        rows.push(ComparisonRow {
            label: key.clone(),
            current: metric.clone(),
            previous: prev,
            trend,
        });
    }

    (rows, has_regression)
}

fn format_metric_value(metric: &Metric) -> String {
    match metric.unit.as_str() {
        "bytes" | "tokens" => format!("{:.0}", metric.value),
        "ms" => format!("{:.2}", metric.value),
        _ => format!("{:.3}", metric.value),
    }
}

fn format_delta(prev: &Metric, curr: &Metric) -> String {
    let diff = curr.value - prev.value;
    match curr.unit.as_str() {
        "bytes" | "tokens" => {
            if prev.value.abs() <= EPSILON {
                format!("{diff:+.0}")
            } else {
                let pct = (diff / prev.value) * 100.0;
                format!("{diff:+.0} ({pct:+.1}%)")
            }
        }
        "ms" => format!("{diff:+.2} ms"),
        _ => format!("{diff:+.3}"),
    }
}

fn print_report_table(report: &BenchReport) {
    crate::out!("Benchmark saispace · {} · {}", report.git_sha, report.date);
    crate::out!("");
    crate::out!("{:<32} {:>10}  {}", "Métrica", "Valor", "Unidad");
    crate::out!("{}", "─".repeat(58));
    let mut seen = HashSet::new();
    for &(key, label) in METRIC_LABELS {
        if let Some(metric) = report.metrics.get(key) {
            seen.insert(key);
            let dir = if metric.higher_is_better {
                "↑"
            } else {
                "↓"
            };
            let info = if metric.informational { ", info" } else { "" };
            crate::out!(
                "{:<32} {:>10}  {} ({}{})",
                label,
                format_metric_value(metric),
                metric.unit,
                dir,
                info
            );
        }
    }
    for (key, metric) in &report.metrics {
        if seen.contains(key.as_str()) {
            continue;
        }
        let dir = if metric.higher_is_better {
            "↑"
        } else {
            "↓"
        };
        crate::out!(
            "{:<32} {:>10}  {} ({})",
            key,
            format_metric_value(metric),
            metric.unit,
            dir
        );
    }
}

fn print_comparison_table(
    current: &BenchReport,
    previous: &BenchReport,
    rows: &[ComparisonRow],
    has_regression: bool,
) {
    crate::out!(
        "Comparación benchmark saispace · {} vs {}",
        current.git_sha,
        previous.git_sha
    );
    crate::out!("");
    crate::out!(
        "{:<32} {:>10} {:>10} {:>16}  {}",
        "Métrica",
        "Actual",
        "Base",
        "Delta",
        "Estado"
    );
    crate::out!("{}", "─".repeat(82));
    for row in rows {
        let cur_s = format_metric_value(&row.current);
        let (base_s, delta_s, status_s) = match &row.previous {
            Some(prev) => {
                let status = match row.trend {
                    MetricTrend::Equal => "= igual",
                    MetricTrend::Better => "+ mejor",
                    MetricTrend::Worse => "- peor",
                    MetricTrend::Regressed => "! REGRESIÓN",
                };
                (
                    format_metric_value(prev),
                    format_delta(prev, &row.current),
                    status,
                )
            }
            None => ("—".to_string(), "nuevo".to_string(), "· sin base"),
        };
        crate::out!(
            "{:<32} {:>10} {:>10} {:>16}  {}",
            row.label,
            cur_s,
            base_s,
            delta_s,
            status_s
        );
    }
    crate::out!("");
    if has_regression {
        crate::out!("FALLO: hay regresiones fuera de umbral.");
    } else {
        crate::out!("OK: sin regresiones respecto a la última línea base.");
    }
}

fn apply_history_and_compare(
    args: &BenchArgs,
    report: &BenchReport,
    history_path: &Path,
) -> CliResult<ExitCode> {
    let previous = if args.compare {
        read_last_history(history_path)?
    } else {
        None
    };

    if args.record {
        append_history(history_path, report)?;
    }

    let mut has_regression = false;
    let comparison = previous.as_ref().map(|prev| {
        let (rows, regressed) = build_comparison(report, prev);
        has_regression = regressed;
        (prev, rows)
    });

    if args.json {
        crate::out!("{}", serde_json::to_string_pretty(report)?);
    } else if let Some((prev, rows)) = &comparison {
        print_comparison_table(report, prev, rows, has_regression);
    } else {
        print_report_table(report);
        if args.compare {
            crate::out!(
                "\nSin historial previo en {}; no hay base para comparar.",
                history_path.display()
            );
        }
        if args.record {
            crate::out!("\nGuardado en {}.", history_path.display());
        }
    }

    Ok(if has_regression {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn execute_with_paths(
    args: &BenchArgs,
    root: &Path,
    fixtures_dir: &Path,
    history_path: &Path,
) -> CliResult<ExitCode> {
    let fixtures = load_fixtures(fixtures_dir)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(evaluate_all(&fixtures, root))?;
    apply_history_and_compare(args, &report, history_path)
}

pub fn run(args: &BenchArgs) -> CliResult<ExitCode> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd);
    let fixtures_dir = match &args.fixtures {
        Some(dir) if dir.is_absolute() => dir.clone(),
        Some(dir) => cwd.join(dir),
        None => root.join("bench/fixtures"),
    };
    let history_path = root.join("bench/history.jsonl");
    execute_with_paths(args, &root, &fixtures_dir, &history_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_recall_f1_on_tiny_inputs() {
        let (p, r, f1) = precision_recall_f1(3, 1, 2);
        assert!((p - 0.75).abs() < 1e-12);
        assert!((r - 0.6).abs() < 1e-12);
        assert!((f1 - (2.0 * 0.75 * 0.6 / 1.35)).abs() < 1e-12);

        assert_eq!(precision_recall_f1(0, 0, 0), (0.0, 0.0, 0.0));
        assert_eq!(precision_recall_f1(0, 3, 2), (0.0, 0.0, 0.0));
    }

    #[test]
    fn search_recall_and_mrr_on_tiny_inputs() {
        let h1 = vec!["k2".into(), "k1".into(), "k3".into()];
        let e1 = vec!["k1".into(), "k4".into()];
        let h2 = vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
            "k5".into(),
        ];
        let e2 = vec!["k5".into()];
        let h3 = vec!["x".into()];
        let e3 = vec!["y".into()];

        let (recall5, mrr) = search_metrics(&[(&h1, &e1), (&h2, &e2), (&h3, &e3)]);
        // Query 1: 1/2 in top-5, first hit at rank 2 (1/2)
        // Query 2: 0/1 in top-5, first hit at rank 6 (1/6)
        // Query 3: 0/1 in top-5, miss (0)
        assert!((recall5 - (0.5 / 3.0)).abs() < 1e-12);
        assert!((mrr - ((0.5 + 1.0 / 6.0) / 3.0)).abs() < 1e-12);
    }

    #[test]
    fn median_ms_handles_odd_even_and_empty() {
        assert_eq!(median_ms(&mut []), 0.0);
        assert_eq!(median_ms(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median_ms(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
    }

    #[test]
    fn compare_thresholds_respect_ratio_size_and_informational_flags() {
        let r_base = ratio_metric(0.90);
        assert_eq!(
            classify_change(&r_base, &ratio_metric(0.89)),
            MetricTrend::Worse
        );
        assert_eq!(
            classify_change(&r_base, &ratio_metric(0.889)),
            MetricTrend::Regressed
        );
        assert_eq!(
            classify_change(&r_base, &ratio_metric(0.92)),
            MetricTrend::Better
        );

        let b_base = size_metric(1000, "bytes");
        assert_eq!(
            classify_change(&b_base, &size_metric(1030, "bytes")),
            MetricTrend::Worse
        );
        assert_eq!(
            classify_change(&b_base, &size_metric(1031, "bytes")),
            MetricTrend::Regressed
        );
        assert_eq!(
            classify_change(&b_base, &size_metric(950, "bytes")),
            MetricTrend::Better
        );

        let t_base = size_metric(200, "tokens");
        assert_eq!(
            classify_change(&t_base, &size_metric(206, "tokens")),
            MetricTrend::Worse
        );
        assert_eq!(
            classify_change(&t_base, &size_metric(207, "tokens")),
            MetricTrend::Regressed
        );

        let info_base = latency_metric(1.0);
        assert_eq!(
            classify_change(&info_base, &latency_metric(50.0)),
            MetricTrend::Worse
        );
    }

    #[test]
    fn build_comparison_flags_regression_only_on_non_informational() {
        let mut prev_metrics = BTreeMap::new();
        prev_metrics.insert("correction_f1".into(), ratio_metric(0.90));
        prev_metrics.insert("index_bytes".into(), size_metric(1000, "bytes"));
        prev_metrics.insert("latency_save_ms_p50".into(), latency_metric(1.0));
        let prev = BenchReport {
            version: 1,
            git_sha: "abc".into(),
            date: "2026-11-26T00:00:00Z".into(),
            metrics: prev_metrics,
        };

        let mut ok_metrics = BTreeMap::new();
        ok_metrics.insert("correction_f1".into(), ratio_metric(0.89));
        ok_metrics.insert("index_bytes".into(), size_metric(1030, "bytes"));
        ok_metrics.insert("latency_save_ms_p50".into(), latency_metric(10.0));
        let ok_cur = BenchReport {
            version: 1,
            git_sha: "def".into(),
            date: "2026-11-26T00:01:00Z".into(),
            metrics: ok_metrics,
        };
        let (_, ok_regressed) = build_comparison(&ok_cur, &prev);
        assert!(!ok_regressed);

        let mut bad_metrics = ok_cur.metrics.clone();
        bad_metrics.insert("index_bytes".into(), size_metric(1035, "bytes"));
        let bad_cur = BenchReport {
            metrics: bad_metrics,
            ..ok_cur
        };
        let (_, bad_regressed) = build_comparison(&bad_cur, &prev);
        assert!(bad_regressed);
    }

    #[test]
    fn compare_exits_zero_without_history_and_one_on_regression() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join("history.jsonl");
        let mut metrics = BTreeMap::new();
        metrics.insert("correction_f1".into(), ratio_metric(0.85));
        metrics.insert("index_bytes".into(), size_metric(2000, "bytes"));
        metrics.insert("latency_context_ms_p50".into(), latency_metric(12.0));
        let report = BenchReport {
            version: 1,
            git_sha: "abc1234".into(),
            date: "2026-11-26T00:00:00Z".into(),
            metrics,
        };

        let no_hist = BenchArgs {
            json: false,
            record: false,
            compare: true,
            fixtures: None,
        };
        assert_eq!(
            apply_history_and_compare(&no_hist, &report, &history).expect("no hist"),
            ExitCode::SUCCESS
        );

        let record = BenchArgs {
            json: false,
            record: true,
            compare: false,
            fixtures: None,
        };
        assert_eq!(
            apply_history_and_compare(&record, &report, &history).expect("record"),
            ExitCode::SUCCESS
        );
        assert_eq!(
            apply_history_and_compare(&no_hist, &report, &history).expect("same compare"),
            ExitCode::SUCCESS
        );

        let mut inflated = read_last_history(&history)
            .expect("read")
            .expect("some report");
        inflated
            .metrics
            .insert("correction_f1".into(), ratio_metric(0.999));
        fs::write(
            &history,
            format!("{}\n", serde_json::to_string(&inflated).expect("json")),
        )
        .expect("write");
        assert_eq!(
            apply_history_and_compare(&no_hist, &report, &history).expect("regress"),
            ExitCode::FAILURE
        );
    }
}
