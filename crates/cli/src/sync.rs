//! `sai sync export|import` (share project knowledge through git) and
//! `sai export obsidian` (Markdown vault built from the memory graph).

use crate::memory::current_project;
use crate::{CliError, CliResult};
use clap::Subcommand;
use saispace_core::db::sync::sync_dir;
use saispace_core::models::{MemoryScope, Project};
use saispace_core::{Db, default_db_path};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum SyncCommand {
    #[command(
        about = "Escribe las memorias del proyecto (no las personales ni globales), enlaces, disparadores y reglas en .saispace/sync/"
    )]
    Export,
    #[command(
        about = "Fusiona .saispace/sync/ en tu base de datos (idempotente; gana lo más reciente)"
    )]
    Import,
}

#[derive(Subcommand)]
pub enum ExportCommand {
    #[command(about = "Exporta memorias como notas de Obsidian con [[enlaces]] del grafo")]
    Obsidian {
        #[arg(help = "Carpeta de destino (p. ej. tu vault)", long)]
        out: PathBuf,
        #[arg(help = "Todas las memorias, no solo las del proyecto y globales", long)]
        all: bool,
    },
}

fn block_on<T>(fut: impl std::future::Future<Output = CliResult<T>>) -> CliResult<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(fut)
}

async fn project(db: &Db) -> CliResult<Project> {
    current_project(db, false).await?.ok_or_else(|| {
        CliError::Usage("esta carpeta no es un proyecto registrado: ejecuta `sai init`".to_string())
    })
}

/// # Errors
///
/// Returns an error if the project cannot be resolved or a file fails.
pub fn sync(cmd: &SyncCommand) -> CliResult<()> {
    block_on(async {
        let db = Db::open(&default_db_path()?).await?;
        let project = project(&db).await?;
        let dir = sync_dir(&project);
        match cmd {
            SyncCommand::Export => {
                let c = db.sync_export(&project, &dir).await?;
                crate::out!(
                    "{}: {} memorias, {} enlaces, {} disparadores, {} reglas. Haz commit de la carpeta.",
                    dir.display(),
                    c.memories,
                    c.links,
                    c.triggers,
                    c.tech_rules
                );
            }
            SyncCommand::Import => {
                if !dir.is_dir() {
                    return Err(CliError::Usage(format!(
                        "no existe {}: haz git pull o sai sync export",
                        dir.display()
                    )));
                }
                let r = db.sync_import(&project, &dir).await?;
                crate::out!(
                    "{} nuevas, {} actualizadas, {} sin cambios; {} enlaces, {} disparadores, {} reglas",
                    r.added,
                    r.updated,
                    r.unchanged,
                    r.links,
                    r.triggers,
                    r.tech_rules
                );
            }
        }
        Ok(())
    })
}

/// # Errors
///
/// Returns an error if the memories cannot be read or the notes written.
pub fn export(cmd: &ExportCommand) -> CliResult<()> {
    let ExportCommand::Obsidian { out, all } = cmd;
    block_on(async {
        let db = Db::open(&default_db_path()?).await?;
        let scope = if *all {
            MemoryScope::All
        } else {
            MemoryScope::for_project(current_project(&db, false).await?.map(|p| p.id))
        };
        let n = db.export_obsidian(scope, out).await?;
        crate::out!("{n} notas en {}", out.display());
        Ok(())
    })
}
