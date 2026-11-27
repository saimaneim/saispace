use crate::{CliError, CliResult};
use chrono::{Duration, Utc};
use clap::{Args, Subcommand};
use saispace_core::{
    Db, default_db_path, kpi_catalog, kpi_events_path_for, metric_applies_to_scope, project_key,
};
use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Args)]
pub struct KpiArgs {
    #[command(subcommand)]
    pub command: Option<KpiCommand>,
    #[arg(help = "Salida en formato JSON", long)]
    pub json: bool,
    #[arg(
        help = "Nombre del proyecto a consultar",
        long,
        conflicts_with = "global"
    )]
    pub project: Option<String>,
    #[arg(help = "Fuerza el ámbito global", long, conflicts_with = "project")]
    pub global: bool,
}

#[derive(Subcommand)]
pub enum KpiCommand {
    #[command(about = "Calcula y guarda la foto diaria de KPIs")]
    Snapshot {
        #[arg(long, hide = true)]
        existing_only: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DeltaTrend {
    Better,
    Worse,
    Same,
    /// The metric has no good direction (a volume): only the delta matters.
    Moved,
}

impl DeltaTrend {
    fn mark(self) -> &'static str {
        match self {
            Self::Better => "✓",
            Self::Worse => "✗",
            Self::Same => "=",
            Self::Moved => "·",
        }
    }
}

#[derive(Debug, Serialize)]
struct KpiReportRow {
    name: &'static str,
    label: &'static str,
    unit: &'static str,
    higher_is_better: Option<bool>,
    value: Option<f64>,
    delta_7d: Option<f64>,
    trend_7d: Option<DeltaTrend>,
    delta_30d: Option<f64>,
    trend_30d: Option<DeltaTrend>,
}

#[derive(Debug, Serialize)]
struct KpiReport {
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    day: String,
    metrics: Vec<KpiReportRow>,
}

struct ResolvedScope {
    scope: String,
    project_name: Option<String>,
}

fn classify_delta(delta: f64, higher_is_better: Option<bool>) -> DeltaTrend {
    let Some(higher_is_better) = higher_is_better else {
        return if delta.abs() < 1e-9 {
            DeltaTrend::Same
        } else {
            DeltaTrend::Moved
        };
    };
    if delta.abs() < 1e-9 {
        DeltaTrend::Same
    } else if (delta > 0.0 && higher_is_better) || (delta < 0.0 && !higher_is_better) {
        DeltaTrend::Better
    } else {
        DeltaTrend::Worse
    }
}

fn format_value(value: f64, unit: &str) -> String {
    match unit {
        "ratio" => format!("{:.1}%", value * 100.0),
        "links_per_memory" => format!("{value:.2}"),
        "bytes" => {
            if value >= 1024.0 {
                format!("{:.1} KB", value / 1024.0)
            } else {
                format!("{value:.0} B")
            }
        }
        _ => format!("{value:.0}"),
    }
}

fn format_delta(delta: f64, unit: &str, trend: DeltaTrend) -> String {
    let sign = if delta > 1e-9 { "+" } else { "" };
    let formatted = match unit {
        "ratio" => format!("{sign}{:.1}%", delta * 100.0),
        "links_per_memory" => format!("{sign}{delta:.2}"),
        "bytes" => {
            if delta.abs() >= 1024.0 {
                format!("{sign}{:.1} KB", delta / 1024.0)
            } else {
                format!("{sign}{delta:.0} B")
            }
        }
        _ => format!("{sign}{delta:.0}"),
    };
    format!("{formatted} {}", trend.mark())
}

async fn resolve_scope(db: &Db, args: &KpiArgs) -> CliResult<ResolvedScope> {
    if args.global {
        return Ok(ResolvedScope {
            scope: "global".to_string(),
            project_name: None,
        });
    }
    if let Some(name) = args.project.as_deref() {
        let projects = db.list_projects().await?;
        if let Ok(target_id) = Uuid::parse_str(name.trim())
            && let Some(project) = projects.iter().find(|p| p.id == target_id)
        {
            return Ok(ResolvedScope {
                scope: project.id.to_string(),
                project_name: Some(project.name.clone()),
            });
        }
        let exact: Vec<_> = projects.iter().filter(|p| p.name == name).collect();
        match exact.as_slice() {
            [project] => {
                return Ok(ResolvedScope {
                    scope: project.id.to_string(),
                    project_name: Some(project.name.clone()),
                });
            }
            [] => {}
            _ => {
                return Err(CliError::Usage(format!(
                    "hay varios proyectos con nombre '{name}'; pasa su id"
                )));
            }
        }
        let needle_key = project_key(name);
        let fuzzy: Vec<_> = projects
            .iter()
            .filter(|p| project_key(&p.name) == needle_key)
            .collect();
        return match fuzzy.as_slice() {
            [project] => Ok(ResolvedScope {
                scope: project.id.to_string(),
                project_name: Some(project.name.clone()),
            }),
            [] => Err(CliError::Usage(format!(
                "proyecto '{name}' no registrado en saispace"
            ))),
            _ => Err(CliError::Usage(format!(
                "hay varios proyectos que coinciden con '{name}'; pasa su id"
            ))),
        };
    }
    let cwd = std::env::current_dir()?;
    let cwd = cwd.canonicalize().unwrap_or(cwd);
    if let Some(project) = db.find_project_for_path(&cwd).await? {
        return Ok(ResolvedScope {
            scope: project.id.to_string(),
            project_name: Some(project.name),
        });
    }
    Ok(ResolvedScope {
        scope: "global".to_string(),
        project_name: None,
    })
}

fn delta_for(
    current: Option<f64>,
    baseline: &BTreeMap<String, f64>,
    metric: &str,
    higher_is_better: Option<bool>,
) -> (Option<f64>, Option<DeltaTrend>) {
    let (Some(curr), Some(prev)) = (current, baseline.get(metric).copied()) else {
        return (None, None);
    };
    let delta = curr - prev;
    (Some(delta), Some(classify_delta(delta, higher_is_better)))
}

async fn run_snapshot(existing_only: bool) -> CliResult<()> {
    let db_path = default_db_path()?;
    if existing_only && !db_path.exists() {
        return Ok(());
    }
    let db = Db::open(&db_path).await?;
    let now = Utc::now();
    let log_path = kpi_events_path_for(&db_path);
    let _ = db.take_daily_snapshot(now, Some(&log_path)).await?;
    Ok(())
}

async fn run_async(args: &KpiArgs) -> CliResult<()> {
    if let Some(KpiCommand::Snapshot { existing_only }) = args.command {
        return run_snapshot(existing_only).await;
    }

    let db_path = default_db_path()?;
    let db = Db::open(&db_path).await?;
    let now = Utc::now();
    let log_path = kpi_events_path_for(&db_path);
    let computation = db.take_daily_snapshot(now, Some(&log_path)).await?;

    let resolved = resolve_scope(&db, args).await?;
    let current_metrics: BTreeMap<String, f64> = if resolved.scope == "global" {
        computation.global.metrics
    } else {
        computation
            .projects
            .into_iter()
            .find(|p| p.scope == resolved.scope)
            .map_or_default(|p| p.metrics)
    };

    let today = now.date_naive();
    let day_7d = (today - Duration::days(7)).format("%Y-%m-%d").to_string();
    let day_30d = (today - Duration::days(30)).format("%Y-%m-%d").to_string();
    let baseline_7d = db.kpi_baseline_before(&resolved.scope, &day_7d).await?;
    let baseline_30d = db.kpi_baseline_before(&resolved.scope, &day_30d).await?;

    let rows: Vec<KpiReportRow> = kpi_catalog()
        .iter()
        .filter(|def| metric_applies_to_scope(def, &resolved.scope))
        .map(|def| {
            let value = current_metrics.get(def.name).copied();
            let (delta_7d, trend_7d) =
                delta_for(value, &baseline_7d, def.name, def.higher_is_better);
            let (delta_30d, trend_30d) =
                delta_for(value, &baseline_30d, def.name, def.higher_is_better);
            KpiReportRow {
                name: def.name,
                label: def.label,
                unit: def.unit,
                higher_is_better: def.higher_is_better,
                value,
                delta_7d,
                trend_7d,
                delta_30d,
                trend_30d,
            }
        })
        .collect();

    if args.json {
        let report = KpiReport {
            scope: resolved.scope,
            project_name: resolved.project_name,
            day: computation.day,
            metrics: rows,
        };
        crate::out!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let scope_label = match resolved.project_name.as_deref() {
        Some(name) => format!("proyecto {name}"),
        None => "global".to_string(),
    };
    crate::out!("# KPIs · {scope_label} · {}", computation.day);
    for row in &rows {
        let val = row
            .value
            .map_or_else(|| "—".to_string(), |v| format_value(v, row.unit));
        let d7 = match (row.delta_7d, row.trend_7d) {
            (Some(d), Some(t)) => format_delta(d, row.unit, t),
            _ => "—".to_string(),
        };
        let d30 = match (row.delta_30d, row.trend_30d) {
            (Some(d), Some(t)) => format_delta(d, row.unit, t),
            _ => "—".to_string(),
        };
        crate::out!(
            "{:<26} {:>9}   7d: {:<11}   30d: {:<11}",
            row.label,
            val,
            d7,
            d30
        );
    }
    Ok(())
}

pub fn run(args: &KpiArgs) -> CliResult<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async(args))
}
