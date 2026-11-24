use crate::models::{Memory, MemoryContext, MemoryKind, Project};
use std::fmt::Write as _;

pub const CONTEXT_BUDGET: usize = 6_000;

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

#[must_use]
pub fn render_context(project: Option<&Project>, context: &MemoryContext) -> String {
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
    if !context.pinned.is_empty() {
        out.push_str("\n## Fijadas\n");
        for m in &context.pinned {
            let _ = writeln!(
                out,
                "- [{}] {} · {}\n  {}",
                kind_label(m.kind),
                m.title,
                m.id,
                excerpt(&m.content, 240)
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
        && context.pinned.is_empty()
        && context.last_session.is_none()
        && context.memories.is_empty()
    {
        out.push_str("\nSin memorias todavía.\n");
    }
    out
}

#[must_use]
pub fn render_memory(m: &Memory) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProjectStatus;
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
            context_notes: String::new(),
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
            });
        }
        let context = MemoryContext {
            memories,
            ..MemoryContext::default()
        };
        let rendered = render_context(None, &context);
        assert!(rendered.contains("- … más con `sai memory search`"));
    }
}
