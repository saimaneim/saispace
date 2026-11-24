use crate::{CliError, CliResult};
use clap::Subcommand;
use saispace_core::models::{
    ActionKind, Actor, EntityKind, Memory, MemoryContext, MemoryKind, MemoryScope, Project,
    RecordActivityInput, SaveMemoryInput,
};
use saispace_core::{Db, default_db_path};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Subcommand)]
pub enum MemoryCommand {
    #[command(about = "Guarda o actualiza (mismo --topic) una memoria del proyecto actual")]
    Save {
        #[arg(long)]
        title: String,
        #[arg(help = "Contenido; usa \"-\" para leerlo de stdin")]
        #[arg(long)]
        content: String,
        #[arg(
            help = "decision, architecture, bugfix, pattern, config, discovery, preference, session_summary, note"
        )]
        #[arg(long, default_value = "note")]
        kind: String,
        #[arg(long)]
        topic: Option<String>,
        #[arg(help = "Memoria global, no ligada al proyecto")]
        #[arg(long)]
        global: bool,
    },
    #[command(about = "Búsqueda FTS5 en memorias, notas, tareas y reglas")]
    Search {
        query: String,
        #[arg(help = "Busca en todos los proyectos")]
        #[arg(long)]
        all: bool,
        #[arg(long, default_value_t = 10)]
        limit: i64,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Índice de memorias del proyecto actual (para el inicio de sesión)")]
    Context {
        #[arg(long, default_value_t = 12)]
        limit: i64,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Contenido completo de una memoria")]
    Get {
        id: Uuid,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Elimina una memoria")]
    Rm { id: Uuid },
    #[command(about = "Importa las observaciones de engram (idempotente)")]
    ImportEngram {
        #[arg(long)]
        db: Option<PathBuf>,
    },
}

fn actor() -> Actor {
    Actor::Agent {
        name: saispace_core::detect_agent_name("sai-cli"),
    }
}

/// Project for the current directory. `register` creates it on first save;
/// reads only pick up folders that imported memories already name.
async fn current_project(db: &Db, register: bool) -> CliResult<Option<Project>> {
    let cwd = std::env::current_dir()?;
    let cwd = cwd.canonicalize().unwrap_or(cwd);
    Ok(db.project_for_path(&cwd, register).await?)
}

/// Hook output above ~10 KB is cut to a 2 KB preview by Claude Code.
const CONTEXT_BUDGET: usize = 6_000;

fn excerpt(text: &str, max: usize) -> String {
    let flat = text
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Decision => "decision",
        MemoryKind::Architecture => "architecture",
        MemoryKind::Bugfix => "bugfix",
        MemoryKind::Pattern => "pattern",
        MemoryKind::Config => "config",
        MemoryKind::Discovery => "discovery",
        MemoryKind::Preference => "preference",
        MemoryKind::SessionSummary => "session_summary",
        MemoryKind::Note => "note",
    }
}

fn render_context(project: Option<&Project>, context: &MemoryContext) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    match project {
        Some(p) => {
            let _ = writeln!(out, "# Memoria de {} ({})", p.name, p.path);
        }
        None => out.push_str("# Memoria global (este directorio aún no tiene memorias propias)\n"),
    }
    out.push_str(
        "Abre una con `sai memory get <id>`; busca con `sai memory search \"<texto>\"`.\n\
         Guarda decisiones, causas raíz y pendientes con `sai memory save --topic <tema>` (mismo tema = actualiza).\n",
    );
    if !context.preferences.is_empty() {
        out.push_str("\n## Preferencias del usuario\n");
        for m in &context.preferences {
            let _ = writeln!(
                out,
                "- {}: {} ({})",
                m.title,
                excerpt(&m.content, 240),
                m.id
            );
        }
    }
    if let Some(m) = &context.last_session {
        let _ = writeln!(
            out,
            "\n## Última sesión · {} · {}\n{}",
            m.updated_at.format("%Y-%m-%d"),
            m.id,
            excerpt(&m.content, 700)
        );
    }
    if !context.memories.is_empty() {
        out.push_str("\n## Memorias\n");
        for m in &context.memories {
            let line = format!(
                "- [{}] {} · {} · {}\n",
                kind_label(m.kind),
                excerpt(&m.title, 90),
                m.updated_at.format("%Y-%m-%d"),
                m.id
            );
            if out.len() + line.len() > CONTEXT_BUDGET {
                out.push_str("- … más con `sai memory search`\n");
                break;
            }
            out.push_str(&line);
        }
    }
    if context.preferences.is_empty()
        && context.last_session.is_none()
        && context.memories.is_empty()
    {
        out.push_str("\nSin memorias todavía.\n");
    }
    out
}

fn render_memory(m: &Memory) -> String {
    format!(
        "# {}\n{} · {} · {} · actualizada {}{}\n\n{}",
        m.title,
        kind_label(m.kind),
        m.project_name.as_deref().unwrap_or("global"),
        m.id,
        m.updated_at.format("%Y-%m-%d %H:%M"),
        m.topic_key
            .as_deref()
            .map_or_else(String::new, |t| format!(" · tema {t}")),
        m.content.trim()
    )
}

fn print_memory_line(m: &Memory) {
    let scope = m.project_name.as_deref().unwrap_or("global");
    crate::out!(
        "{}  {}  [{:?}] {}  ({scope})",
        m.id,
        m.updated_at.format("%Y-%m-%d"),
        m.kind,
        m.title
    );
}

pub fn run(cmd: MemoryCommand) -> CliResult<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async(cmd))
}

async fn save(
    db: &Db,
    title: String,
    content: String,
    kind: &str,
    topic: Option<String>,
    global: bool,
) -> CliResult<()> {
    let kind = MemoryKind::parse(kind)
        .ok_or_else(|| CliError::Usage(format!("unknown memory kind '{kind}'")))?;
    let content = if content == "-" {
        std::io::read_to_string(std::io::stdin())?
    } else {
        content
    };
    let project = if global {
        None
    } else {
        let project = current_project(db, true).await?;
        if project.is_none() {
            return Err(CliError::Usage(
                "este directorio no es un proyecto (¿$HOME o ~/Dev?): entra en la carpeta del proyecto o usa --global para conocimiento entre proyectos".to_string(),
            ));
        }
        project
    };
    let saved = db
        .save_memory(SaveMemoryInput {
            project_id: project.as_ref().map(|p| p.id),
            project_name: None,
            kind,
            title,
            content,
            topic_key: topic,
        })
        .await?;
    let action = if saved.created_at == saved.updated_at {
        ActionKind::Create
    } else {
        ActionKind::Update
    };
    db.record_activity(RecordActivityInput {
        project_id: saved.project_id,
        actor: actor(),
        action,
        entity: EntityKind::Memory,
        entity_id: Some(saved.id),
        details: Some(serde_json::json!({ "title": saved.title })),
    })
    .await?;
    print_memory_line(&saved);
    let similar = db.similar_memories(&saved, 3).await?;
    if !similar.is_empty() {
        crate::out!(
            "parecidas (actualiza con el mismo --topic o borra la sobrante con `sai memory rm <id>`):"
        );
        for h in &similar {
            crate::out!("  {}  {}", h.id, h.title);
        }
    }
    Ok(())
}

async fn run_async(cmd: MemoryCommand) -> CliResult<()> {
    let db = Db::open(&default_db_path()?).await?;
    match cmd {
        MemoryCommand::Save {
            title,
            content,
            kind,
            topic,
            global,
        } => save(&db, title, content, &kind, topic, global).await?,
        MemoryCommand::Search {
            query,
            all,
            limit,
            json,
        } => {
            let scope = if all {
                MemoryScope::All
            } else {
                MemoryScope::for_project(current_project(&db, false).await?.map(|p| p.id))
            };
            let hits = db.search_scoped(&query, scope, Some(limit)).await?;
            if json {
                crate::out!("{}", serde_json::to_string_pretty(&hits)?);
            } else {
                for h in &hits {
                    crate::out!(
                        "{:?}  {}  {}\n    {}",
                        h.kind,
                        h.id,
                        h.title,
                        excerpt(&h.snippet, 200)
                    );
                }
                if hits.is_empty() {
                    crate::out!("sin resultados");
                }
            }
        }
        MemoryCommand::Context { limit, json } => {
            let project = current_project(&db, false).await?;
            let scope = MemoryScope::for_project(project.as_ref().map(|p| p.id));
            let context = db.memory_context(scope, limit).await?;
            if json {
                crate::out!("{}", serde_json::to_string_pretty(&context)?);
            } else {
                crate::out!("{}", render_context(project.as_ref(), &context).trim_end());
            }
        }
        MemoryCommand::Get { id, json } => {
            let memory = db.get_memory(id).await?;
            if json {
                crate::out!("{}", serde_json::to_string_pretty(&memory)?);
            } else {
                crate::out!("{}", render_memory(&memory));
            }
        }
        MemoryCommand::Rm { id } => {
            db.delete_memory(id).await?;
            crate::out!("eliminada {id}");
        }
        MemoryCommand::ImportEngram { db: source } => {
            let source = match source {
                Some(path) => path,
                None => std::env::var_os("HOME")
                    .map(|h| PathBuf::from(h).join(".engram").join("engram.db"))
                    .ok_or_else(|| CliError::Usage("HOME is not set".to_string()))?,
            };
            let report = db.import_engram(&source).await?;
            crate::out!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}
