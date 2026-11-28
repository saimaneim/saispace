//! `sai doctor`: read-only health report (database, index, vault key, agents).

use crate::CliResult;
use saispace_core::default_db_path;
use std::process::ExitCode;

#[derive(clap::Args)]
pub struct DoctorArgs {
    /// Full `integrity_check` instead of the quick one.
    #[arg(long)]
    deep: bool,
    #[arg(long)]
    json: bool,
}

fn mib(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // display only
    let value = bytes as f64 / 1_048_576.0;
    format!("{value:.1} MiB")
}

/// # Errors
///
/// Returns an error if the database cannot be opened.
pub fn run(args: &DoctorArgs) -> CliResult<ExitCode> {
    let db_path = default_db_path()?;
    let cwd = std::env::current_dir()?;
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from);
    let report = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(saispace_core::doctor::run(
            &db_path,
            crate::VERSION,
            &cwd,
            home.as_deref(),
            args.deep,
        ))?;
    if args.json {
        crate::out!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        crate::out!("sai {}", report.version);
        crate::out!(
            "base de datos: {} ({}, WAL {})",
            report.db_path,
            mib(report.db_bytes),
            mib(report.wal_bytes)
        );
        crate::out!(
            "proyecto: {}",
            report
                .project
                .as_deref()
                .unwrap_or("ninguno en esta carpeta")
        );
        for c in &report.checks {
            crate::out!(
                "{} {}: {}",
                if c.ok { "ok " } else { "MAL" },
                c.name,
                c.detail
            );
        }
    }
    Ok(if report.healthy() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
