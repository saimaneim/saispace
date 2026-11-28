use crate::{CliError, CliResult};
use clap::Subcommand;
use saispace_core::memory_render::{
    excerpt, kind_label, render_compact_notice, render_context_within, render_memory,
};
use saispace_core::models::{
    ActionKind, Actor, ConsolidateInput, EntityKind, LinkDirection, Memory, MemoryKind,
    MemoryRelation, MemoryScope, Project, RecordActivityInput, SaveMemoryInput, SearchKind,
    UpdateMemoryInput, Visibility,
};
use saispace_core::{Db, default_db_path, short_id};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
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
        #[arg(
            help = "Disparador que la recuerda sola: texto del error (\"exit code 144\") o env:t3code / env:opencode. Repetible",
            long = "trigger"
        )]
        triggers: Vec<String>,
        #[arg(
            help = "Caduca en esta fecha o plazo (2026-12-01, 2026-12-01T10:00:00Z, 6h, 3d): hechos temporales como una cuota agotada",
            long
        )]
        until: Option<String>,
        #[arg(help = "Nota personal: no se comparte con `sai sync export`", long)]
        personal: bool,
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
        #[arg(help = "Id de la memoria (completo o los 8 últimos dígitos)")]
        id: String,
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
        #[arg(
            help = "Reemplaza los disparadores (texto de error o env:<host>). Repetible",
            long = "trigger"
        )]
        triggers: Vec<String>,
        #[arg(
            help = "Quita todos los disparadores",
            long,
            conflicts_with = "triggers"
        )]
        no_triggers: bool,
        #[arg(
            help = "Caduca en esta fecha o plazo (2026-12-01, 2026-12-01T10:00:00Z, 6h, 3d): hechos temporales como una cuota agotada",
            long
        )]
        until: Option<String>,
        #[arg(help = "Quita la caducidad", long, conflicts_with = "until")]
        no_until: bool,
        #[arg(help = "Márcala como personal (no se exporta)", long)]
        personal: bool,
        #[arg(
            help = "Márcala como del proyecto (se exporta)",
            long,
            conflicts_with = "personal"
        )]
        shared: bool,
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
        #[arg(
            help = "Tamaño máximo en bytes (por defecto SAISPACE_CONTEXT_BYTES u 8000)",
            long
        )]
        max_bytes: Option<usize>,
    },
    #[command(about = "Contenido completo de una memoria")]
    Get {
        #[arg(help = "Id de la memoria (completo o los 8 últimos dígitos)")]
        id: String,
        #[arg(help = "Salida en formato JSON", long)]
        json: bool,
    },
    #[command(about = "Fija una memoria")]
    Pin { id: String },
    #[command(about = "Desfija una memoria")]
    Unpin { id: String },
    #[command(about = "Elimina una memoria", visible_alias = "delete")]
    Rm {
        #[arg(help = "Id de la memoria a eliminar")]
        id: String,
    },
    #[command(
        about = "Borra una memoria sin dejar rastro (índice, enlaces, actividad y páginas libres). Irreversible"
    )]
    Purge {
        #[arg(help = "Id de la memoria a purgar")]
        id: String,
        #[arg(help = "Confirma el borrado irreversible", long)]
        yes: bool,
    },
    #[command(about = "Enlaza dos memorias (relacionada, depende de)")]
    Connect {
        #[arg(help = "Memoria origen")]
        from: String,
        #[arg(help = "Memoria destino")]
        to: String,
        #[arg(
            help = "Relación: related, depends_on",
            long,
            default_value = "related"
        )]
        relation: String,
    },
    #[command(about = "Quita los enlaces entre dos memorias")]
    Disconnect { a: String, b: String },
    #[command(
        about = "Une varias memorias en una: la superviviente recibe el texto nuevo y las demás quedan archivadas"
    )]
    Merge {
        #[arg(help = "Ids de las memorias a unir (2 o más)", num_args = 2.., required = true)]
        ids: Vec<String>,
        #[arg(help = "Título de la memoria unida", long)]
        title: String,
        #[arg(help = "Contenido unido; usa \"-\" para leerlo de stdin", long)]
        content: String,
        #[arg(help = "Tipo (por defecto, el de la superviviente)", long)]
        kind: Option<String>,
        #[arg(help = "Tema (por defecto, el que ya tuviera alguna)", long)]
        topic: Option<String>,
    },
    #[command(about = "Detecta posibles memorias duplicadas")]
    Dupes {
        #[arg(help = "Busca en todos los proyectos")]
        #[arg(long)]
        all: bool,
    },
    #[command(about = "Revisa memorias sin uso y resúmenes de sesión sobrantes")]
    Stale {
        #[arg(
            help = "Días sin uso ni edición para marcar una memoria",
            long,
            default_value_t = 60
        )]
        days: i64,
        #[arg(
            help = "Resúmenes de sesión a conservar por proyecto",
            long,
            default_value_t = 3
        )]
        keep_sessions: i64,
        #[arg(help = "Revisa todos los proyectos", long)]
        all: bool,
        #[arg(help = "Salida en formato JSON", long)]
        json: bool,
        #[arg(help = "Elimina solo los resúmenes de sesión sobrantes", long)]
        prune_sessions: bool,
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
    #[command(
        about = "Normaliza los topic_key guardados a la convención actual (migración de datos)"
    )]
    NormalizeTopics,
    #[command(about = "Enlaza las memorias existentes que se mencionan entre sí (por id o tema)")]
    Relink,
    #[command(about = "Hook para detectar correcciones y recordar guardar lecciones en memoria")]
    Nudge,
}

fn actor() -> Actor {
    Actor::Agent {
        name: saispace_core::detect_agent_name("sai-cli"),
    }
}

/// Project for the current directory. `register` creates it on first save;
/// reads only pick up folders that imported memories already name.
pub(crate) async fn current_project(db: &Db, register: bool) -> CliResult<Option<Project>> {
    current_project_at(db, register, std::env::current_dir()?).await
}

/// Project for an explicit directory, used when Claude Code's hook passes a
/// `cwd` that overrides the process working directory.
pub(crate) async fn current_project_at(
    db: &Db,
    register: bool,
    cwd: PathBuf,
) -> CliResult<Option<Project>> {
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
        short_id(m.id),
        m.updated_at.format("%Y-%m-%d"),
        m.kind,
        m.title
    );
}

pub fn run(cmd: MemoryCommand) -> CliResult<()> {
    let runtime = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
    };
    if matches!(cmd, MemoryCommand::Nudge) {
        // Tool events need the database; the other hooks answer from stdin.
        if let Some(event) = crate::nudge::run() {
            runtime()?.block_on(crate::recall::run(event));
        }
        return Ok(());
    }
    runtime()?.block_on(run_async(cmd))
}

async fn save(
    db: &Db,
    title: String,
    content: String,
    kind: &str,
    topic: Option<String>,
    global: bool,
    triggers: &[String],
) -> CliResult<Uuid> {
    // Validate before saving, so a bad trigger never leaves a half-saved memory.
    saispace_core::recall::normalize_triggers(triggers).map_err(CliError::Usage)?;
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
    if !triggers.is_empty() {
        let set = db.set_memory_triggers(saved.id, triggers).await?;
        crate::out!("disparadores: {}", set.join(" · "));
    }
    let linked: Vec<String> = db
        .memory_links(saved.id)
        .await?
        .iter()
        .filter(|l| l.direction == LinkDirection::Out && l.relation == MemoryRelation::Related)
        .map(|l| format!("{} {}", short_id(l.id), l.title))
        .collect();
    if !linked.is_empty() {
        crate::out!("enlazada con: {}", linked.join(" · "));
    }
    let similar = db.similar_memories(&saved, 3).await?;
    if !similar.is_empty() {
        crate::out!(
            "parecidas (actualiza con el mismo --topic o únelas con `sai memory merge <id> <id>`):"
        );
        for s in &similar {
            // Reusing a topic only updates within the same scope: a global
            // memory's key saved from a project would create a new row.
            if s.topic_key.is_empty() || s.hit.project_id != saved.project_id {
                crate::out!("  {}  {}", short_id(s.hit.id), s.hit.title);
            } else {
                crate::out!(
                    "  {}  {}  (usa --topic {} para actualizarla)",
                    short_id(s.hit.id),
                    s.hit.title,
                    s.topic_key
                );
            }
        }
    }
    Ok(saved.id)
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
            // Memories print their short id; tasks and rules keep the full
            // one, which their own commands expect.
            let id = if h.kind == SearchKind::Memory {
                short_id(h.id)
            } else {
                h.id.to_string()
            };
            crate::out!(
                "{:?}  {}  {}\n    {}",
                h.kind,
                id,
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

async fn merge(
    db: &Db,
    raw_ids: &[String],
    title: String,
    content: String,
    kind: Option<&str>,
    topic: Option<String>,
) -> CliResult<()> {
    let mut ids = Vec::with_capacity(raw_ids.len());
    for raw in raw_ids {
        ids.push(db.resolve_memory_id(raw).await?);
    }
    let kind = kind
        .map(|k| {
            MemoryKind::parse(k)
                .ok_or_else(|| CliError::Usage(format!("unknown memory kind '{k}'")))
        })
        .transpose()?;
    let content = if content == "-" {
        std::io::read_to_string(std::io::stdin())?
    } else {
        content
    };
    let merged = db
        .consolidate_memories(ConsolidateInput {
            ids: ids.clone(),
            title,
            content,
            kind,
            topic_key: topic,
        })
        .await?;
    db.record_activity(RecordActivityInput {
        project_id: merged.project_id,
        actor: actor(),
        action: ActionKind::Update,
        entity: EntityKind::Memory,
        entity_id: Some(merged.id),
        details: Some(serde_json::json!({ "title": merged.title, "consolidated": ids.len() })),
    })
    .await?;
    print_memory_line(&merged);
    crate::out!(
        "archivadas en ella: {}",
        ids.iter()
            .filter(|id| **id != merged.id)
            .map(|id| short_id(*id))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

async fn rm(db: &Db, id: Uuid) -> CliResult<()> {
    let memory = db.get_memory(id).await?;
    db.delete_memory(id).await?;
    crate::out!("eliminada {}  {}", short_id(memory.id), memory.title);
    Ok(())
}

async fn stale(
    db: &Db,
    days: i64,
    keep_sessions: i64,
    all: bool,
    json: bool,
    prune_sessions: bool,
) -> CliResult<()> {
    let scope = if all {
        MemoryScope::All
    } else {
        MemoryScope::for_project(current_project(db, false).await?.map(|p| p.id))
    };
    let mut report = db.stale_report(scope, days, keep_sessions).await?;
    if prune_sessions {
        for m in &report.surplus_sessions {
            db.delete_memory(m.id).await?;
            db.record_activity(RecordActivityInput {
                project_id: m.project_id,
                actor: actor(),
                action: ActionKind::Delete,
                entity: EntityKind::Memory,
                entity_id: Some(m.id),
                details: Some(serde_json::json!({ "title": m.title })),
            })
            .await?;
            if !json {
                crate::out!("eliminada {}  {}", m.id, m.title);
            }
        }
        report.surplus_sessions.clear();
    }
    if json {
        crate::out!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if report.stale.is_empty() && report.surplus_sessions.is_empty() {
        crate::out!("nada que revisar");
        return Ok(());
    }
    crate::out!("# Sin uso en {days} días · {}", report.stale.len());
    if report.stale.is_empty() {
        crate::out!("ninguna");
    } else {
        for m in &report.stale {
            print_memory_line(m);
        }
        crate::out!("revísala con `sai memory get <id>` o elimínala con `sai memory rm <id>`");
    }
    crate::out!(
        "\n# Resúmenes de sesión sobrantes · {}",
        report.surplus_sessions.len()
    );
    if report.surplus_sessions.is_empty() {
        crate::out!("ninguno");
    } else {
        for m in &report.surplus_sessions {
            print_memory_line(m);
        }
    }
    Ok(())
}

async fn edit(
    db: &Db,
    id: Uuid,
    title: Option<String>,
    content: Option<String>,
    kind: Option<&str>,
    topic: Option<String>,
    triggers: Option<&[String]>,
) -> CliResult<()> {
    let fields_changed = title.is_some() || content.is_some() || kind.is_some() || topic.is_some();
    if !fields_changed && triggers.is_none() {
        return Err(CliError::Usage(
            "indica al menos un campo a modificar (--title, --content, --kind, --topic, --trigger, --no-triggers)"
                .to_string(),
        ));
    }
    if let Some(list) = triggers {
        saispace_core::recall::normalize_triggers(list).map_err(CliError::Usage)?;
    }
    if !fields_changed {
        let memory = db.get_memory(id).await?;
        if let Some(list) = triggers {
            let set = db.set_memory_triggers(id, list).await?;
            print_memory_line(&memory);
            crate::out!(
                "disparadores: {}",
                if set.is_empty() {
                    "ninguno".to_string()
                } else {
                    set.join(" · ")
                }
            );
        }
        return Ok(());
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
    if let Some(list) = triggers {
        let set = db.set_memory_triggers(id, list).await?;
        crate::out!(
            "disparadores: {}",
            if set.is_empty() {
                "ninguno".to_string()
            } else {
                set.join(" · ")
            }
        );
    }
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
    crate::out!(
        "{} {}",
        if pinned { "fijada" } else { "desfijada" },
        short_id(id)
    );
    Ok(())
}

fn spawn_kpi_snapshot(db_path: &Path) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["kpi", "snapshot", "--existing-only"])
        .env("SAISPACE_DB_PATH", db_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    let _ = cmd.spawn();
}

/// Lets one process per day start the snapshot: sessions that start together
/// (startup + compact, or two projects) would otherwise each run a full
/// computation. Claims from earlier days are removed on the way.
fn claim_snapshot_day(db_path: &Path, day: &str) -> bool {
    let Some(dir) = db_path.parent() else {
        return false;
    };
    let claim = dir.join(format!("kpi-snapshot.{day}.claim"));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("kpi-snapshot.")
                && name.ends_with(".claim")
                && entry.path() != claim
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&claim)
        .is_ok()
}

async fn context(
    db: &Db,
    db_path: &Path,
    limit: i64,
    json: bool,
    source: Option<String>,
    max_bytes: Option<usize>,
) -> CliResult<()> {
    let budget = max_bytes.unwrap_or_else(saispace_core::memory_render::context_budget_from_env);
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
    let mut context = db.memory_context(scope, limit).await?;
    let hosts = saispace_core::recall::detect_hosts(|key| std::env::var(key).ok());
    db.attach_environment(&mut context, scope, &hosts).await?;
    if let Some(p) = &project {
        db.attach_open_tasks(&mut context, p.id).await?;
    }
    db.attach_temporary(&mut context, chrono::Utc::now())
        .await?;
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
            render_context_within(project.as_ref(), &context, budget).trim_end()
        );
    } else {
        crate::out!(
            "{}",
            render_context_within(project.as_ref(), &context, budget).trim_end()
        );
    }
    let _ = std::io::stdout().flush();

    let now = chrono::Utc::now();
    let day = now.format("%Y-%m-%d").to_string();
    let scope_id = project
        .as_ref()
        .map_or_else(|| "global".to_string(), |p| p.id.to_string());
    if matches!(db.has_snapshot_for_day(&day, &scope_id).await, Ok(false))
        && claim_snapshot_day(db_path, &day)
    {
        spawn_kpi_snapshot(db_path);
    }
    Ok(())
}

async fn get(db: &Db, id: &str, json: bool) -> CliResult<()> {
    let memory = db.get_memory(db.resolve_memory_id(id).await?).await?;
    let links = db.memory_links(memory.id).await?;
    let triggers = db.memory_triggers(memory.id).await?;
    touch_memories_or_warn(db, &[memory.id]).await;
    if json {
        let value = serde_json::json!({ "memory": memory, "links": links, "triggers": triggers });
        crate::out!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        crate::out!("{}", render_memory(&memory, &links).trim_end());
        if !triggers.is_empty() {
            crate::out!("\nDisparadores: {}", triggers.join(" · "));
        }
    }
    Ok(())
}

async fn connect(db: &Db, from: &str, to: &str, relation: &str) -> CliResult<()> {
    let relation = MemoryRelation::parse(relation)
        .filter(|r| *r != MemoryRelation::Supersedes)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "relación '{relation}' no válida: usa related o depends_on (merge crea supersedes)"
            ))
        })?;
    let from = db.resolve_memory_id(from).await?;
    let to = db.resolve_memory_id(to).await?;
    let added = db.link_memories(from, to, relation).await?;
    crate::out!(
        "{} {} → {} ({})",
        if added {
            "enlazadas"
        } else {
            "ya estaban enlazadas"
        },
        short_id(from),
        short_id(to),
        relation.as_str()
    );
    Ok(())
}

async fn dupes(db: &Db, all: bool) -> CliResult<()> {
    let scope = if all {
        MemoryScope::All
    } else {
        MemoryScope::for_project(current_project(db, false).await?.map(|p| p.id))
    };
    let pairs = db.duplicate_pairs(scope, 50).await?;
    if pairs.is_empty() {
        crate::out!("sin duplicados");
    } else {
        for pair in &pairs {
            crate::out!("{}  {}", short_id(pair.a.id), pair.a.title);
            crate::out!("  ≈ {}  {}", short_id(pair.b.id), pair.b.title);
        }
        crate::out!("únelas con `sai memory merge <id> <id> --title … --content -`");
    }
    Ok(())
}

async fn import_engram(db: &Db, source: Option<PathBuf>) -> CliResult<()> {
    let source = match source {
        Some(path) => path,
        None => std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".engram").join("engram.db"))
            .ok_or_else(|| CliError::Usage("HOME is not set".to_string()))?,
    };
    let report = db.import_engram(&source).await?;
    crate::out!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Sets (`Some`) or clears (`None`) a memory's expiry and prints it.
async fn set_until(db: &Db, id: Uuid, until: Option<&str>) -> CliResult<()> {
    let at = until
        .map(|raw| saispace_core::db::memory_expiry::parse_until(raw, chrono::Utc::now()))
        .transpose()?;
    db.set_memory_expiry(id, at).await?;
    crate::out!(
        "caduca: {}",
        at.map_or_else(
            || "nunca".to_string(),
            |at| at.format("%Y-%m-%d %H:%M UTC").to_string()
        )
    );
    Ok(())
}

#[allow(clippy::too_many_lines)] // one arm per subcommand
async fn run_async(cmd: MemoryCommand) -> CliResult<()> {
    let db_path = default_db_path()?;
    let db = Db::open(&db_path).await?;
    match cmd {
        MemoryCommand::Save {
            title,
            content,
            kind,
            topic,
            global,
            triggers,
            until,
            personal,
        } => {
            // Parse first so a bad date never leaves a half-saved memory.
            if let Some(raw) = &until {
                saispace_core::db::memory_expiry::parse_until(raw, chrono::Utc::now())?;
            }
            let id = save(&db, title, content, &kind, topic, global, &triggers).await?;
            if until.is_some() {
                set_until(&db, id, until.as_deref()).await?;
            }
            if personal {
                db.set_memory_scope(id, Visibility::Personal).await?;
            }
        }
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
            triggers,
            no_triggers,
            until,
            no_until,
            personal,
            shared,
        } => {
            let id = db.resolve_memory_id(&id).await?;
            if personal || shared {
                let scope = if personal {
                    Visibility::Personal
                } else {
                    Visibility::Project
                };
                db.set_memory_scope(id, scope).await?;
            }
            let expiry_only = title.is_none()
                && content.is_none()
                && kind.is_none()
                && topic.is_none()
                && triggers.is_empty()
                && !no_triggers;
            if until.is_some() || no_until {
                set_until(&db, id, until.as_deref()).await?;
            }
            if expiry_only && (until.is_some() || no_until || personal || shared) {
                return Ok(());
            }
            // --no-triggers conflicts with --trigger, so it means an empty list.
            let triggers = (no_triggers || !triggers.is_empty()).then_some(triggers);
            edit(
                &db,
                id,
                title,
                content,
                kind.as_deref(),
                topic,
                triggers.as_deref(),
            )
            .await?;
        }
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
            max_bytes,
        } => context(&db, &db_path, limit, json, source, max_bytes).await?,
        MemoryCommand::Get { id, json } => get(&db, &id, json).await?,
        MemoryCommand::Rm { id } => rm(&db, db.resolve_memory_id(&id).await?).await?,
        MemoryCommand::Purge { id, yes } => {
            let id = db.resolve_memory_id(&id).await?;
            let memory = db.get_memory(id).await?;
            if !yes {
                return Err(CliError::Usage(format!(
                    "purgar {} «{}» es irreversible: repite con --yes",
                    short_id(id),
                    memory.title
                )));
            }
            db.purge_memory(id).await?;
            crate::out!("purgada {}", short_id(id));
        }
        MemoryCommand::Pin { id } => {
            set_pinned(&db, db.resolve_memory_id(&id).await?, true).await?;
        }
        MemoryCommand::Unpin { id } => {
            set_pinned(&db, db.resolve_memory_id(&id).await?, false).await?;
        }
        MemoryCommand::Connect { from, to, relation } => {
            connect(&db, &from, &to, &relation).await?;
        }
        MemoryCommand::Disconnect { a, b } => {
            let a = db.resolve_memory_id(&a).await?;
            let b = db.resolve_memory_id(&b).await?;
            let removed = db.unlink_memories(a, b).await?;
            crate::out!("{removed} enlaces quitados");
        }
        MemoryCommand::Merge {
            ids,
            title,
            content,
            kind,
            topic,
        } => merge(&db, &ids, title, content, kind.as_deref(), topic).await?,
        MemoryCommand::Dupes { all } => dupes(&db, all).await?,
        MemoryCommand::Stale {
            days,
            keep_sessions,
            all,
            json,
            prune_sessions,
        } => stale(&db, days, keep_sessions, all, json, prune_sessions).await?,
        MemoryCommand::ImportEngram { db: source } => import_engram(&db, source).await?,
        MemoryCommand::Orphans => list_orphans(&db).await?,
        MemoryCommand::Link { project_name, path } => {
            link_orphans(&db, &project_name, path).await?;
        }
        MemoryCommand::Relink => {
            let added = db.relink_all().await?;
            crate::out!("{added} enlaces nuevos");
        }
        MemoryCommand::NormalizeTopics => {
            let changed = db.normalize_existing_topic_keys().await?;
            crate::out!("{changed} claves de tema normalizadas");
        }
        MemoryCommand::Nudge => {}
    }
    Ok(())
}
