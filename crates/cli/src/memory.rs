use crate::{CliError, CliResult};
use clap::Subcommand;
use saispace_core::memory_render::{
    excerpt, kind_label, render_compact_notice, render_context, render_memory,
};
use saispace_core::models::{
    ActionKind, Actor, EntityKind, Memory, MemoryKind, MemoryScope, Project, RecordActivityInput,
    SaveMemoryInput, SearchKind, UpdateMemoryInput,
};
use saispace_core::{Db, default_db_path};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Subcommand)]
pub enum MemoryCommand {
    #[command(about = "Guarda o actualiza (mismo --topic) una memoria del proyecto actual")]
    Save {
        #[arg(help = "Título de la memoria", long)]
        title: String,
        #[arg(help = "Contenido; usa \"-\" para leerlo de stdin", long)]
        content: String,
        #[arg(
            help = "Tipo: decision, architecture, bugfix, pattern, config, discovery, preference, session_summary, note",
            long,
            default_value = "note"
        )]
        kind: String,
        #[arg(help = "Tema para agrupar o actualizar la memoria", long)]
        topic: Option<String>,
        #[arg(help = "Memoria global, no ligada al proyecto", long)]
        global: bool,
    },
    #[command(about = "Lista memorias recientes del proyecto y globales")]
    List {
        #[arg(
            help = "Filtra por tipo (decision, architecture, bugfix, pattern, config, discovery, preference, session_summary, note)",
            long
        )]
        kind: Option<String>,
        #[arg(help = "Muestra memorias de todos los proyectos", long)]
        all: bool,
        #[arg(help = "Límite de memorias a mostrar", long, default_value_t = 50)]
        limit: i64,
        #[arg(help = "Salida en formato JSON", long)]
        json: bool,
    },
    #[command(about = "Edita una memoria existente")]
    Edit {
        id: Uuid,
        #[arg(long)]
        title: Option<String>,
        #[arg(help = "Contenido; usa \"-\" para leerlo de stdin")]
        #[arg(long)]
        content: Option<String>,
        #[arg(
            help = "decision, architecture, bugfix, pattern, config, discovery, preference, session_summary, note"
        )]
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        topic: Option<String>,
    },
    #[command(about = "Búsqueda FTS5 en memorias, notas, tareas y reglas")]
    Search {
        #[arg(help = "Texto de búsqueda")]
        query: String,
        #[arg(help = "Busca en todos los proyectos", long)]
        all: bool,
        #[arg(help = "Límite de resultados", long, default_value_t = 10)]
        limit: i64,
        #[arg(help = "Salida en formato JSON", long)]
        json: bool,
    },
    #[command(about = "Índice de memorias del proyecto actual (para el inicio de sesión)")]
    Context {
        #[arg(help = "Límite de memorias en el índice", long, default_value_t = 12)]
        limit: i64,
        #[arg(help = "Salida en formato JSON", long)]
        json: bool,
        #[arg(
            help = "Origen de la sesión (startup, resume, clear, compact)",
            long,
            value_parser = ["startup", "resume", "clear", "compact"]
        )]
        source: Option<String>,
    },
    #[command(about = "Contenido completo de una memoria")]
    Get {
        #[arg(help = "Identificador de la memoria")]
        id: Uuid,
        #[arg(help = "Salida en formato JSON", long)]
        json: bool,
    },
    #[command(about = "Fija una memoria")]
    Pin { id: Uuid },
    #[command(about = "Desfija una memoria")]
    Unpin { id: Uuid },
    #[command(about = "Elimina una memoria")]
    Rm {
        #[arg(help = "Identificador de la memoria a eliminar")]
        id: Uuid,
    },
    #[command(about = "Detecta posibles memorias duplicadas")]
    Dupes {
        #[arg(help = "Busca en todos los proyectos")]
        #[arg(long)]
        all: bool,
    },
    #[command(about = "Importa las observaciones de engram (idempotente)")]
    ImportEngram {
        #[arg(help = "Ruta opcional a la base de datos de engram", long)]
        db: Option<PathBuf>,
    },
    #[command(about = "Vincula memorias huérfanas a un proyecto")]
    Link {
        project_name: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    #[command(about = "Lista grupos de memorias sin proyecto")]
    Orphans,
}

fn actor() -> Actor {
    Actor::Agent {
        name: saispace_core::detect_agent_name("sai-cli"),
    }
}

/// Project for the current directory. `register` creates it on first save;
/// reads only pick up folders that imported memories already name.
async fn current_project(db: &Db, register: bool) -> CliResult<Option<Project>> {
    current_project_at(db, register, std::env::current_dir()?).await
}

/// Project for an explicit directory, used when Claude Code's hook passes a
/// `cwd` that overrides the process working directory.
async fn current_project_at(db: &Db, register: bool, cwd: PathBuf) -> CliResult<Option<Project>> {
    let cwd = cwd.canonicalize().unwrap_or(cwd);
    Ok(db.project_for_path(&cwd, register).await?)
}

/// Upper bound on how much stdin we read when sniffing for the Claude Code
/// hook payload, so a stray pipe can't feed us unbounded data.
const STDIN_MAX_BYTES: u64 = 64 * 1024;

struct HookInput {
    source: Option<String>,
    cwd: Option<PathBuf>,
}

/// Reads the Claude Code hook JSON from stdin. Claude Code runs
/// `sai memory context` on `SessionStart` (including after compaction) and
/// always closes stdin once the hook payload is written, so reading never
/// blocks. When stdin is a TTY (an interactive shell) we skip it entirely, so
/// the command never hangs waiting for input that won't come. A missing,
/// empty or non-JSON payload is ignored silently.
fn read_hook_input() -> Option<HookInput> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut buf = Vec::new();
    stdin
        .lock()
        .take(STDIN_MAX_BYTES)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&buf).ok()?;
    let source = value
        .get("source")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let cwd = value
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .filter(|p| p.is_dir());
    Some(HookInput { source, cwd })
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

async fn list(db: &Db, kind: Option<String>, all: bool, limit: i64, json: bool) -> CliResult<()> {
    let target_kind = match kind.as_deref() {
        Some(raw) => Some(
            MemoryKind::parse(raw)
                .ok_or_else(|| CliError::Usage(format!("unknown memory kind '{raw}'")))?,
        ),
        None => None,
    };
    let project = current_project(db, false).await?;
    let scope = if all {
        MemoryScope::All
    } else {
        MemoryScope::for_project(project.as_ref().map(|p| p.id))
    };
    let fetch_limit = if target_kind.is_some() { 200 } else { limit };
    let mut memories = db.recent_memories(scope, fetch_limit).await?;
    if let Some(target) = target_kind {
        memories.retain(|m| m.kind == target);
        let max = usize::try_from(limit.max(0)).unwrap_or(0);
        memories.truncate(max);
    }
    if json {
        crate::out!("{}", serde_json::to_string_pretty(&memories)?);
        return Ok(());
    }
    if memories.is_empty() {
        crate::out!("Sin memorias.");
        return Ok(());
    }
    let header = if all {
        format!("# Memorias de todos los proyectos · {}", memories.len())
    } else if let Some(p) = &project {
        format!("# Memorias de {} · {}", p.name, memories.len())
    } else {
        format!("# Memorias globales · {}", memories.len())
    };
    crate::out!("{header}");
    let mut groups: Vec<(MemoryKind, Vec<&Memory>)> = Vec::new();
    for m in &memories {
        if let Some((_, list)) = groups.iter_mut().find(|(k, _)| *k == m.kind) {
            list.push(m);
        } else {
            groups.push((m.kind, vec![m]));
        }
    }
    for (kind, items) in groups {
        crate::out!("\n## {}", kind_label(kind));
        for m in items {
            print_memory_line(m);
        }
    }
    Ok(())
}

async fn touch_memories_or_warn(db: &Db, ids: &[Uuid]) {
    if let Err(err) = db.touch_memories(ids).await {
        eprintln!("aviso: no se pudo actualizar el uso de memoria: {err}");
    }
}

async fn search(db: &Db, query: &str, all: bool, limit: i64, json: bool) -> CliResult<()> {
    let scope = if all {
        MemoryScope::All
    } else {
        MemoryScope::for_project(current_project(db, false).await?.map(|p| p.id))
    };
    let hits = db.search_scoped(query, scope, Some(limit)).await?;
    let memory_ids: Vec<Uuid> = hits
        .iter()
        .filter(|h| h.kind == SearchKind::Memory)
        .map(|h| h.id)
        .collect();
    touch_memories_or_warn(db, &memory_ids).await;
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
    Ok(())
}

async fn rm(db: &Db, id: Uuid) -> CliResult<()> {
    let memory = db.get_memory(id).await?;
    db.delete_memory(id).await?;
    crate::out!("eliminada {}  {}", memory.id, memory.title);
    Ok(())
}

async fn edit(
    db: &Db,
    id: Uuid,
    title: Option<String>,
    content: Option<String>,
    kind: Option<&str>,
    topic: Option<String>,
) -> CliResult<()> {
    if title.is_none() && content.is_none() && kind.is_none() && topic.is_none() {
        return Err(CliError::Usage(
            "indica al menos un campo a modificar (--title, --content, --kind, --topic)"
                .to_string(),
        ));
    }
    let parsed_kind = match kind {
        Some(k) => Some(
            MemoryKind::parse(k)
                .ok_or_else(|| CliError::Usage(format!("unknown memory kind '{k}'")))?,
        ),
        None => None,
    };
    let content = match content {
        Some(c) if c == "-" => Some(std::io::read_to_string(std::io::stdin())?),
        Some(c) => Some(c),
        None => None,
    };
    let updated = db
        .update_memory(
            id,
            UpdateMemoryInput {
                title,
                content,
                kind: parsed_kind,
                topic_key: topic,
            },
        )
        .await?;
    db.record_activity(RecordActivityInput {
        project_id: updated.project_id,
        actor: actor(),
        action: ActionKind::Update,
        entity: EntityKind::Memory,
        entity_id: Some(updated.id),
        details: Some(serde_json::json!({ "title": updated.title })),
    })
    .await?;
    print_memory_line(&updated);
    Ok(())
}

async fn list_orphans(db: &Db) -> CliResult<()> {
    let groups = db.orphan_memory_groups().await?;
    if groups.is_empty() {
        crate::out!("sin memorias huérfanas");
    } else {
        for g in &groups {
            crate::out!(
                "{}  ({} {})  {}",
                g.project_name,
                g.count,
                if g.count == 1 { "memoria" } else { "memorias" },
                g.last_updated.format("%Y-%m-%d %H:%M")
            );
        }
    }
    Ok(())
}

async fn link_orphans(db: &Db, project_name: &str, path: Option<PathBuf>) -> CliResult<()> {
    let target = match path {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let target = target.canonicalize().unwrap_or(target);
    let project = db.project_for_path(&target, true).await?.ok_or_else(|| {
        CliError::Usage(
            "este directorio no es un proyecto: no se pudo resolver ni registrar".to_string(),
        )
    })?;
    let linked = db.link_orphans_to_project(project_name, project.id).await?;
    db.record_activity(RecordActivityInput {
        project_id: Some(project.id),
        actor: actor(),
        action: ActionKind::Update,
        entity: EntityKind::Memory,
        entity_id: None,
        details: Some(serde_json::json!({
            "project_name": project_name,
            "linked": linked,
        })),
    })
    .await?;
    crate::out!("{linked} memorias vinculadas a {}", project.name);
    Ok(())
}

async fn set_pinned(db: &Db, id: Uuid, pinned: bool) -> CliResult<()> {
    let memory = db.set_memory_pinned(id, pinned).await?;
    db.record_activity(RecordActivityInput {
        project_id: memory.project_id,
        actor: actor(),
        action: ActionKind::Update,
        entity: EntityKind::Memory,
        entity_id: Some(id),
        details: Some(serde_json::json!({ "title": memory.title, "pinned": pinned })),
    })
    .await?;
    crate::out!("{} {id}", if pinned { "fijada" } else { "desfijada" });
    Ok(())
}

async fn context(db: &Db, limit: i64, json: bool, source: Option<String>) -> CliResult<()> {
    // An explicit --source flag wins; otherwise sniff the hook payload that
    // Claude Code pipes on stdin.
    let hook = if source.is_some() {
        None
    } else {
        read_hook_input()
    };
    let source = source.or_else(|| hook.as_ref().and_then(|h| h.source.clone()));
    let is_compact = source.as_deref() == Some("compact");
    let project = match hook.as_ref().and_then(|h| h.cwd.clone()) {
        Some(cwd) => current_project_at(db, false, cwd).await?,
        None => current_project(db, false).await?,
    };
    let scope = MemoryScope::for_project(project.as_ref().map(|p| p.id));
    let context = db.memory_context(scope, limit).await?;
    if json {
        let mut value = serde_json::to_value(&context)?;
        if is_compact && let Some(obj) = value.as_object_mut() {
            obj.insert(
                "notice".to_string(),
                serde_json::Value::String(render_compact_notice().trim_end().to_string()),
            );
        }
        crate::out!("{}", serde_json::to_string_pretty(&value)?);
    } else if is_compact {
        crate::out!(
            "{}\n{}",
            render_compact_notice(),
            render_context(project.as_ref(), &context).trim_end()
        );
    } else {
        crate::out!("{}", render_context(project.as_ref(), &context).trim_end());
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
        MemoryCommand::List {
            kind,
            all,
            limit,
            json,
        } => list(&db, kind, all, limit, json).await?,
        MemoryCommand::Edit {
            id,
            title,
            content,
            kind,
            topic,
        } => edit(&db, id, title, content, kind.as_deref(), topic).await?,
        MemoryCommand::Search {
            query,
            all,
            limit,
            json,
        } => search(&db, &query, all, limit, json).await?,
        MemoryCommand::Context {
            limit,
            json,
            source,
        } => context(&db, limit, json, source).await?,
        MemoryCommand::Get { id, json } => {
            let memory = db.get_memory(id).await?;
            touch_memories_or_warn(&db, &[memory.id]).await;
            if json {
                crate::out!("{}", serde_json::to_string_pretty(&memory)?);
            } else {
                crate::out!("{}", render_memory(&memory));
            }
        }
        MemoryCommand::Rm { id } => rm(&db, id).await?,
        MemoryCommand::Pin { id } => set_pinned(&db, id, true).await?,
        MemoryCommand::Unpin { id } => set_pinned(&db, id, false).await?,
        MemoryCommand::Dupes { all } => {
            let scope = if all {
                MemoryScope::All
            } else {
                MemoryScope::for_project(current_project(&db, false).await?.map(|p| p.id))
            };
            let pairs = db.duplicate_pairs(scope, 50).await?;
            if pairs.is_empty() {
                crate::out!("sin duplicados");
            } else {
                for pair in &pairs {
                    crate::out!("{}  {}", pair.a.id, pair.a.title);
                    crate::out!("  ≈ {}  {}", pair.b.id, pair.b.title);
                }
            }
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
        MemoryCommand::Orphans => list_orphans(&db).await?,
        MemoryCommand::Link { project_name, path } => {
            link_orphans(&db, &project_name, path).await?;
        }
    }
    Ok(())
}
