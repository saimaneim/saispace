//! `sai export obsidian`: one Markdown note per memory with YAML frontmatter
//! and real `[[wikilinks]]` built from the memory graph, so Obsidian's graph
//! view shows the same connections agents follow.

use crate::db::Db;
use crate::db::memory::MEMORY_COLUMNS;
use crate::db::memory_graph::short_id;
use crate::db::sync::SyncLink;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, MemoryScope};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// File name (without `.md`) of a memory: its title, made safe for every OS,
/// plus the short id so equal titles never collide.
#[must_use]
pub fn note_name(memory: &Memory) -> String {
    let safe: String = memory
        .title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let words: Vec<&str> = safe.split_whitespace().collect();
    let mut title = words.join(" ");
    if title.chars().count() > 80 {
        title = title
            .chars()
            .take(80)
            .collect::<String>()
            .trim_end()
            .to_string();
    }
    format!("{title} ({})", short_id(memory.id))
}

fn yaml(value: &str) -> String {
    // A JSON string is a valid YAML double-quoted scalar.
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn kind_name(m: &Memory) -> String {
    serde_json::to_value(m.kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn render_note(m: &Memory, links: &[(String, String)]) -> String {
    let kind = kind_name(m);
    let mut out = String::from("---\n");
    let _ = writeln!(out, "id: {}", m.id);
    let _ = writeln!(out, "kind: {}", yaml(&kind));
    if let Some(p) = &m.project_name {
        let _ = writeln!(out, "project: {}", yaml(p));
    }
    if let Some(t) = &m.topic_key {
        let _ = writeln!(out, "topic_key: {}", yaml(t));
    }
    let _ = writeln!(out, "created: {}", m.created_at.to_rfc3339());
    let _ = writeln!(out, "updated: {}", m.updated_at.to_rfc3339());
    if m.pinned {
        out.push_str("pinned: true\n");
    }
    let _ = writeln!(out, "tags: [{}]", yaml(&format!("saispace/{kind}")));
    let _ = writeln!(out, "---\n\n# {}\n\n{}", m.title, m.content.trim_end());
    if !links.is_empty() {
        out.push_str("\n## Enlaces\n");
        for (relation, target) in links {
            let _ = writeln!(out, "- {relation}: [[{target}]]");
        }
    }
    out
}

impl Db {
    /// Writes active memories in `scope` to `out` and returns how many.
    ///
    /// # Errors
    ///
    /// Returns an error if the memories cannot be read or a file written.
    pub async fn export_obsidian(&self, scope: MemoryScope, out: &Path) -> CoreResult<usize> {
        let io = |e: std::io::Error| CoreError::Validation(format!("{}: {e}", out.display()));
        let (mode, project_id) = match scope {
            MemoryScope::All => (0, None),
            MemoryScope::GlobalOnly => (1, None),
            MemoryScope::Project(id) => (2, Some(id)),
        };
        let sql = format!(
            "SELECT {MEMORY_COLUMNS} FROM memories
             WHERE superseded_by IS NULL
               AND (?1 = 0 OR project_id IS NULL OR (?1 = 2 AND project_id = ?2))
             ORDER BY id"
        );
        let memories: Vec<Memory> = sqlx::query_as(&sql)
            .bind(mode)
            .bind(project_id)
            .fetch_all(self.pool())
            .await?;
        let names: HashMap<Uuid, String> = memories.iter().map(|m| (m.id, note_name(m))).collect();
        let edges: Vec<SyncLink> =
            sqlx::query_as("SELECT from_id, to_id, relation FROM memory_links ORDER BY relation")
                .fetch_all(self.pool())
                .await?;
        let mut links: HashMap<Uuid, Vec<(String, String)>> = HashMap::new();
        for e in edges {
            let (Some(from), Some(to)) = (names.get(&e.from_id), names.get(&e.to_id)) else {
                continue;
            };
            links
                .entry(e.from_id)
                .or_default()
                .push((e.relation.replace('_', " "), to.clone()));
            links.entry(e.to_id).or_default().push((
                format!("{} (entrante)", e.relation.replace('_', " ")),
                from.clone(),
            ));
        }
        fs::create_dir_all(out).map_err(io)?;
        for m in &memories {
            let note = render_note(m, links.get(&m.id).map_or(&[][..], Vec::as_slice));
            fs::write(out.join(format!("{}.md", names[&m.id])), note).map_err(io)?;
        }
        Ok(memories.len())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use crate::models::{MemoryKind, MemoryRelation, MemoryScope, SaveMemoryInput};

    #[tokio::test]
    async fn notes_carry_frontmatter_and_wikilinks_both_ways() {
        let db = Db::connect_in_memory().await.expect("db");
        db.migrate().await.expect("migrate");
        let save = |title: &str| SaveMemoryInput {
            project_id: None,
            project_name: None,
            kind: MemoryKind::Decision,
            title: title.into(),
            content: "Cuerpo".into(),
            topic_key: None,
        };
        let a = db.save_memory(save("Usar WAL: lectores")).await.expect("a");
        let b = db.save_memory(save("Pool de 5")).await.expect("b");
        db.link_memories(a.id, b.id, MemoryRelation::DependsOn)
            .await
            .expect("link");

        let out = tempfile::tempdir().expect("tempdir");
        let n = db
            .export_obsidian(MemoryScope::All, out.path())
            .await
            .expect("export");
        assert_eq!(n, 2);
        let a_note =
            std::fs::read_to_string(out.path().join(format!("{}.md", super::note_name(&a))))
                .expect("note a");
        assert!(a_note.starts_with("---\nid: "), "{a_note}");
        assert!(a_note.contains("kind: \"decision\""), "{a_note}");
        assert!(
            a_note.contains(&format!("[[{}]]", super::note_name(&b))),
            "{a_note}"
        );
        assert!(!super::note_name(&a).contains(':'));
        let b_note =
            std::fs::read_to_string(out.path().join(format!("{}.md", super::note_name(&b))))
                .expect("note b");
        assert!(b_note.contains("(entrante)"), "{b_note}");
    }
}
