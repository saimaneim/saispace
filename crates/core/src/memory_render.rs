use crate::db::memory_graph::short_id;
use crate::models::{
    Memory, MemoryContext, MemoryKind, MemoryLink, MemoryRelation, Project, TaskStatus,
};
use std::fmt::Write as _;

/// Default size cap of the session context, in bytes. Sections are filled in
/// priority order (preferences, environment, temporary, pinned, plan, last
/// session, memories) and stop when the cap is reached.
pub const CONTEXT_BUDGET: usize = 8_000;
/// Smallest accepted cap: enough for the header and the top preferences.
pub const MIN_CONTEXT_BUDGET: usize = 1_000;

/// Context budget from `SAISPACE_CONTEXT_BYTES`, or the default.
#[must_use]
pub fn context_budget_from_env() -> usize {
    std::env::var("SAISPACE_CONTEXT_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map_or(CONTEXT_BUDGET, |n| n.max(MIN_CONTEXT_BUDGET))
}

const CUT_NOTE: &str = "- … más con `sai memory search`\n";

/// Appends `line` while it fits in `budget`; once one line does not fit,
/// every later line is dropped and a single cut note is written.
struct Budget {
    max: usize,
    full: bool,
}

impl Budget {
    fn push(&mut self, out: &mut String, line: &str) -> bool {
        if self.full {
            return false;
        }
        if out.len() + line.len() + CUT_NOTE.len() > self.max {
            self.full = true;
            out.push_str(CUT_NOTE);
            return false;
        }
        out.push_str(line);
        true
    }
}

/// Notice prepended to the session-start index after Claude Code compacts the
/// conversation, reminding the agent to persist what it worked on.
pub const COMPACT_NOTICE: &str = "## Primera acción tras compactar\nGuarda un `session_summary` con lo trabajado antes de compactar (Goal, Discoveries, Accomplished, Next steps): `save_memory` con kind session_summary, o `sai memory save --kind session_summary`. Usa el mismo topic_key de la sesión si ya guardaste uno.\n";

#[must_use]
pub fn render_compact_notice() -> &'static str {
    COMPACT_NOTICE
}

#[must_use]
pub fn excerpt(text: &str, max: usize) -> String {
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

/// The rule itself, without the "What:" label nor the "Why:" rationale:
/// the agent needs the rule every session, the reason only when it opens it.
#[must_use]
pub fn rule_preview(text: &str, max: usize) -> String {
    let flat = excerpt(text, usize::MAX);
    let body = flat
        .strip_prefix("What:")
        .or_else(|| flat.strip_prefix("**What**:"))
        .unwrap_or(&flat)
        .trim_start();
    let rule = ["· Why:", "· **Why**:", " Why:"]
        .iter()
        .filter_map(|marker| body.find(marker))
        .min()
        .map_or(body, |cut| &body[..cut])
        .trim_end_matches([' ', '·']);
    excerpt(rule, max)
}

#[must_use]
pub fn kind_label(kind: MemoryKind) -> &'static str {
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

/// Host lessons (`env:` triggers), shown in full: they apply right away.
fn render_environment(out: &mut String, context: &MemoryContext) {
    if !context.environment.is_empty() {
        let hosts: Vec<&str> = context
            .hosts
            .iter()
            .map(|h| crate::recall::host_label(h))
            .collect();
        let _ = writeln!(
            out,
            "\n## Entorno: {}\nLecciones de este entorno: aplícalas sin esperar a que el usuario lo recuerde.",
            hosts.join(" + ")
        );
        for m in &context.environment {
            let _ = writeln!(
                out,
                "- {}: {} [{}]",
                excerpt(&m.title, 90),
                crate::recall::lesson_text(&m.content, 300),
                short_id(m.id)
            );
        }
    }
}

fn render_temporary(out: &mut String, context: &MemoryContext) {
    if context.temporary.is_empty() {
        return;
    }
    out.push_str("\n## Estado temporal\nHechos con caducidad: tenlos en cuenta antes de actuar (p. ej. no delegues a un proveedor sin cuota).\n");
    for t in &context.temporary {
        let _ = writeln!(
            out,
            "- {}: {} · hasta {} [{}]",
            t.memory.title,
            rule_preview(&t.memory.content, 160),
            t.expires_at.format("%Y-%m-%d %H:%M UTC"),
            short_id(t.memory.id)
        );
    }
}

fn render_open_tasks(out: &mut String, context: &MemoryContext) {
    if context.open_tasks.is_empty() {
        return;
    }
    out.push_str(
        "\n## Pendiente del plan\nTareas sin cerrar. Termínalas o di al usuario por qué no; al acabar una, `sai plan done <id>` (o complete_task).\n",
    );
    for t in &context.open_tasks {
        let status = match t.status {
            TaskStatus::InProgress => " (en curso)",
            TaskStatus::Blocked => " (bloqueada)",
            _ => "",
        };
        let goal = t
            .goal
            .as_deref()
            .map_or_default(|g| format!(" · {}", excerpt(g, 50)));
        let _ = writeln!(
            out,
            "- {}{status}{goal} · {}",
            excerpt(&t.title, 90),
            short_id(t.id)
        );
    }
}

#[must_use]
pub fn render_context(project: Option<&Project>, context: &MemoryContext) -> String {
    render_context_within(project, context, CONTEXT_BUDGET)
}

/// [`render_context`] capped at `max_bytes` (at least [`MIN_CONTEXT_BUDGET`]).
#[must_use]
#[allow(clippy::too_many_lines)] // one block per section, in priority order
pub fn render_context_within(
    project: Option<&Project>,
    context: &MemoryContext,
    max_bytes: usize,
) -> String {
    let mut out = String::new();
    let mut budget = Budget {
        max: max_bytes.max(MIN_CONTEXT_BUDGET),
        full: false,
    };
    match project {
        Some(p) => {
            let _ = writeln!(out, "# Memoria de {} ({})", p.name, p.path);
        }
        None => out.push_str("# Memoria global (este directorio aún no tiene memorias propias)\n"),
    }
    out.push_str(
        "`sai memory get <id>` abre una (con sus enlaces); `sai memory search \"<texto>\"` busca.\n\
         Guarda decisiones, causas raíz y pendientes con `sai memory save --topic <tema>` (mismo tema = actualiza). Si el usuario te corrige, guárdalo con `--kind preference`.\n",
    );
    if !context.preferences.is_empty() {
        out.push_str("\n## Preferencias del usuario\n");
        for m in &context.preferences {
            let line = format!(
                "- {}: {} [{}]\n",
                m.title,
                rule_preview(&m.content, 200),
                short_id(m.id)
            );
            budget.push(&mut out, &line);
        }
    }
    if !budget.full {
        render_environment(&mut out, context);
        render_temporary(&mut out, context);
    }
    if !context.pinned.is_empty() && !budget.full {
        out.push_str("\n## Fijadas\n");
        for m in &context.pinned {
            let line = format!(
                "- [{}] {} · {}\n  {}\n",
                kind_label(m.kind),
                m.title,
                short_id(m.id),
                excerpt(&m.content, 240)
            );
            budget.push(&mut out, &line);
        }
    }
    if !budget.full {
        render_open_tasks(&mut out, context);
    }
    if let Some(m) = &context.last_session {
        let block = format!(
            "\n## Última sesión · {} · {}\n{}\n",
            m.updated_at.format("%Y-%m-%d"),
            short_id(m.id),
            excerpt(&m.content, 700)
        );
        budget.push(&mut out, &block);
    }
    if !context.memories.is_empty() && !budget.full {
        out.push_str("\n## Memorias\n");
        for m in &context.memories {
            let line = format!(
                "- [{}] {} · {} · {}\n",
                kind_label(m.kind),
                excerpt(&m.title, 90),
                m.updated_at.format("%Y-%m-%d"),
                short_id(m.id)
            );
            if !budget.push(&mut out, &line) {
                break;
            }
        }
    }
    if !context.consolidate.is_empty() && !budget.full {
        out.push_str(
            "\n## Para consolidar\nParecen la misma memoria: si lo son, únelas antes de terminar con `consolidate_memories` o `sai memory merge` (no se borra nada).\n",
        );
        for hint in &context.consolidate {
            let line = format!(
                "- {} {} ≈ {} {}\n",
                short_id(hint.a),
                excerpt(&hint.a_title, 60),
                short_id(hint.b),
                excerpt(&hint.b_title, 60)
            );
            if !budget.push(&mut out, &line) {
                break;
            }
        }
    }
    if context.preferences.is_empty()
        && context.environment.is_empty()
        && context.pinned.is_empty()
        && context.last_session.is_none()
        && context.memories.is_empty()
        && context.open_tasks.is_empty()
        && context.temporary.is_empty()
    {
        out.push_str("\nSin memorias todavía.\n");
    }
    out
}

#[must_use]
pub fn relation_label(link: &MemoryLink) -> &'static str {
    use crate::models::LinkDirection::{In, Out};
    match (link.relation, link.direction) {
        (MemoryRelation::Related, _) => "relacionada",
        (MemoryRelation::DependsOn, Out) => "depende de",
        (MemoryRelation::DependsOn, In) => "la necesita",
        (MemoryRelation::Supersedes, Out) => "absorbió",
        (MemoryRelation::Supersedes, In) => "absorbida por",
    }
}

#[must_use]
pub fn render_memory(m: &Memory, links: &[MemoryLink]) -> String {
    let mut out = format!(
        "# {}\n{} · {} · {} · actualizada {}{}{}\n\n{}",
        m.title,
        kind_label(m.kind),
        m.project_name.as_deref().unwrap_or("global"),
        short_id(m.id),
        m.updated_at.format("%Y-%m-%d %H:%M"),
        m.topic_key
            .as_deref()
            .map_or_else(String::new, |t| format!(" · tema {t}")),
        m.superseded_by.map_or_else(String::new, |by| format!(
            " · archivada: consolidada en {}",
            short_id(by)
        )),
        m.content.trim()
    );
    if !links.is_empty() {
        out.push_str("\n\n## Enlaces\n");
        for link in links {
            let _ = writeln!(
                out,
                "- {} {} [{}]{}",
                relation_label(link),
                link.title,
                short_id(link.id),
                if link.archived { " (archivada)" } else { "" }
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProjectStatus, Visibility};
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_excerpt_flattens_newlines() {
        let input = "Primera línea\nSegunda línea\n\nTercera línea";
        assert_eq!(
            excerpt(input, 100),
            "Primera línea · Segunda línea · Tercera línea"
        );
    }

    #[test]
    fn test_excerpt_strips_leading_hashes() {
        let input = "# Título principal\n## Subtítulo con detalles";
        assert_eq!(
            excerpt(input, 100),
            "Título principal · Subtítulo con detalles"
        );
    }

    #[test]
    fn test_excerpt_truncates_with_ellipsis() {
        let input = "Esta es una frase relativamente larga para recortar";
        assert_eq!(excerpt(input, 10), "Esta es un…");
    }

    #[test]
    fn test_render_context_empty_state() {
        let context = MemoryContext::default();
        let rendered = render_context(None, &context);
        assert!(rendered.contains("Sin memorias todavía."));
    }

    #[test]
    fn test_render_context_header_with_project() {
        let project = Project {
            id: Uuid::now_v7(),
            name: "mi-proyecto".to_string(),
            path: "/home/user/mi-proyecto".to_string(),
            status: ProjectStatus::Active,
            pinned: false,
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let context = MemoryContext::default();
        let rendered = render_context(Some(&project), &context);
        assert!(rendered.contains("# Memoria de mi-proyecto (/home/user/mi-proyecto)"));
        assert!(!rendered.contains("# Memoria global"));
    }

    #[test]
    fn test_render_context_header_without_project() {
        let context = MemoryContext::default();
        let rendered = render_context(None, &context);
        assert!(
            rendered.contains("# Memoria global (este directorio aún no tiene memorias propias)")
        );
    }

    #[test]
    fn test_render_context_budget_cap() {
        let mut memories = Vec::new();
        let now = Utc::now();
        for i in 0..150 {
            memories.push(Memory {
                id: Uuid::now_v7(),
                project_id: None,
                project_name: None,
                kind: MemoryKind::Decision,
                title: format!("Decisión muy importante número {i} con descripción extensa para ocupar presupuesto"),
                content: "Contenido de prueba".to_string(),
                topic_key: None,
                pinned: false,
                created_at: now,
                updated_at: now,
                last_used_at: None,
                use_count: 0,
                superseded_by: None,
                scope: Visibility::Project,
            });
        }
        let context = MemoryContext {
            memories,
            ..MemoryContext::default()
        };
        let rendered = render_context(None, &context);
        assert!(rendered.contains("- … más con `sai memory search`"));
    }

    #[test]
    fn context_never_exceeds_its_budget_and_keeps_preferences_first() {
        let prefs: Vec<Memory> = (0..40)
            .map(|i| memory(&format!("Preferencia {i}"), &"texto largo ".repeat(20)))
            .collect();
        let context = MemoryContext {
            preferences: prefs,
            ..MemoryContext::default()
        };
        let rendered = render_context_within(None, &context, 2_000);
        assert!(rendered.len() <= 2_000, "{}", rendered.len());
        assert!(rendered.contains("Preferencia 0"));
        assert_eq!(rendered.matches("… más con").count(), 1);
    }

    fn memory(title: &str, content: &str) -> Memory {
        let now = Utc::now();
        Memory {
            id: Uuid::now_v7(),
            project_id: None,
            project_name: None,
            kind: MemoryKind::Preference,
            title: title.to_string(),
            content: content.to_string(),
            topic_key: None,
            pinned: false,
            created_at: now,
            updated_at: now,
            last_used_at: None,
            use_count: 0,
            superseded_by: None,
            scope: Visibility::Project,
        }
    }

    #[test]
    fn rule_preview_keeps_only_the_rule() {
        assert_eq!(
            rule_preview("What: commits en inglés\nWhy: lo pide el equipo", 200),
            "commits en inglés"
        );
        assert_eq!(rule_preview("Sin etiquetas", 200), "Sin etiquetas");
    }

    #[test]
    fn consolidate_hints_render_with_short_ids_and_an_instruction() {
        let a = memory("Auth con cookies", "x");
        let b = memory("Auth cookies v2", "y");
        let context = MemoryContext {
            memories: vec![a.clone(), b.clone()],
            consolidate: vec![crate::models::ConsolidateHint {
                a: a.id,
                a_title: a.title.clone(),
                b: b.id,
                b_title: b.title.clone(),
            }],
            ..MemoryContext::default()
        };
        let rendered = render_context(None, &context);
        assert!(rendered.contains("## Para consolidar"));
        assert!(rendered.contains("consolidate_memories"));
        assert!(rendered.contains(&format!(
            "- {} Auth con cookies ≈ {} Auth cookies v2",
            short_id(a.id),
            short_id(b.id)
        )));
    }

    #[test]
    fn render_memory_lists_links_and_archive_state() {
        let survivor = memory("Superviviente", "x");
        let mut archived = memory("Vieja", "y");
        archived.superseded_by = Some(survivor.id);
        let links = vec![MemoryLink {
            id: survivor.id,
            title: survivor.title.clone(),
            kind: MemoryKind::Preference,
            relation: MemoryRelation::Supersedes,
            direction: crate::models::LinkDirection::In,
            archived: false,
        }];
        let rendered = render_memory(&archived, &links);
        assert!(rendered.contains(&format!(
            "archivada: consolidada en {}",
            short_id(survivor.id)
        )));
        assert!(rendered.contains(&format!(
            "- absorbida por Superviviente [{}]",
            short_id(survivor.id)
        )));
    }
}
