//! `PostToolUse` / `PostToolUseFailure`: when a tool call shows an error (or a
//! command) that a saved lesson names in its triggers, hand that lesson to the
//! agent before it retries. Each lesson shows once per session.

use crate::nudge::{HookPayload, append_kpi_event, now_rfc3339, session_marker_path_in};
use saispace_core::recall::{MAX_RECALL_HITS, detect_hosts, normalize_text, render_recall};
use saispace_core::{Db, MemoryScope, default_db_path};
use serde_json::Value;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Bytes of tool input and output scanned: the head and the tail, where
/// commands print their errors.
const HAYSTACK_EDGE_BYTES: usize = 16 * 1024;
const MARKER_RETENTION: Duration = Duration::from_hours(7 * 24);

/// Tools whose successful output is file or search content: matching it would
/// recall lessons whenever the agent reads code that mentions an error.
const CONTENT_TOOLS: &[&str] = &[
    "Read",
    "Edit",
    "Write",
    "MultiEdit",
    "NotebookEdit",
    "NotebookRead",
    "Grep",
    "Glob",
    "LS",
    "TodoWrite",
    "Task",
    "Agent",
    "WebFetch",
    "WebSearch",
    "read",
    "edit",
    "write",
    "grep",
    "glob",
    "list",
    "todowrite",
    "todoread",
    "webfetch",
    "websearch",
    "task",
    "patch",
    "apply_patch",
    "multiedit",
];

/// Memory tools of any MCP alias: their input and output quote lessons.
const MEMORY_TOOL_SUFFIXES: &[&str] = &[
    "save_memory",
    "search_memory",
    "get_memory",
    "memory_context",
    "consolidate_memories",
    "link_memories",
];

fn collect_strings(value: &Value, out: &mut String) {
    match value {
        Value::String(s) => {
            out.push_str(s);
            out.push('\n');
        }
        Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}

fn edges(text: &str) -> String {
    if text.len() <= 2 * HAYSTACK_EDGE_BYTES {
        return text.to_string();
    }
    let mut head = HAYSTACK_EDGE_BYTES;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = text.len() - HAYSTACK_EDGE_BYTES;
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!("{}\n{}", &text[..head], &text[tail..])
}

/// Normalized text a trigger is matched against, or `None` when the call must
/// not recall anything (saispace's own tools, saving a lesson, reading files).
pub(crate) fn haystack(payload: &HookPayload) -> Option<String> {
    let tool = payload.tool_name.as_deref().unwrap_or_default();
    if tool.contains("saispace") || MEMORY_TOOL_SUFFIXES.iter().any(|s| tool.ends_with(s)) {
        return None;
    }
    let mut input = String::new();
    if let Some(v) = &payload.tool_input {
        collect_strings(v, &mut input);
    }
    let content_tool = CONTENT_TOOLS.contains(&tool);
    // Saving or editing a lesson repeats its trigger text: never self-recall.
    if !content_tool && input.contains("sai memory") {
        return None;
    }
    let mut text = String::new();
    // A content tool's input is file text (an edit quoting an error); its
    // output is file or search content: only its error says what went wrong.
    if !content_tool {
        text.push_str(&input);
    }
    if !content_tool && let Some(v) = &payload.tool_response {
        collect_strings(v, &mut text);
    }
    if let Some(v) = &payload.error {
        collect_strings(v, &mut text);
    }
    let text = normalize_text(&edges(&text));
    (!text.is_empty()).then_some(text)
}

fn shown_ids(path: &PathBuf) -> Vec<Uuid> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| Uuid::parse_str(l.trim()).ok())
        .collect()
}

fn remember_shown(path: &PathBuf, ids: &[Uuid]) {
    let Some(dir) = path.parent() else { return };
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    prune_old_markers(dir);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        for id in ids {
            let _ = writeln!(file, "{id}");
        }
    }
}

fn prune_old_markers(dir: &std::path::Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > MARKER_RETENTION);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

pub async fn run(payload: HookPayload) {
    let Some(haystack) = haystack(&payload) else {
        return;
    };
    // Without a session the once-per-session guard cannot work.
    let Some(session) = payload.session() else {
        return;
    };
    let Ok(db_path) = default_db_path() else {
        return;
    };
    // Never create the database from a hook: no database, no lessons.
    if !db_path.exists() {
        return;
    }
    let Ok(db) = Db::open(&db_path).await else {
        return;
    };
    let project = match payload
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
    {
        Some(cwd) => crate::memory::current_project_at(&db, false, cwd).await,
        None => crate::memory::current_project(&db, false).await,
    };
    let scope = MemoryScope::for_project(project.ok().flatten().map(|p| p.id));
    let hosts = detect_hosts(|key| std::env::var(key).ok());
    let Ok(found) = db.triggered_memories(scope, &haystack, &hosts, false).await else {
        return;
    };
    let marker = session_marker_path_in("recalled", session);
    let already = marker.as_ref().map_or_else(Vec::new, shown_ids);
    let hits: Vec<_> = found
        .into_iter()
        .filter(|m| !already.contains(&m.id))
        .take(MAX_RECALL_HITS)
        .collect();
    if hits.is_empty() {
        return;
    }
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": payload.event(),
            "additionalContext": render_recall(&hits),
        }
    });
    let Ok(line) = serde_json::to_string(&output) else {
        return;
    };
    crate::out!("{line}");
    let ids: Vec<Uuid> = hits.iter().map(|m| m.id).collect();
    if let Some(path) = &marker {
        remember_shown(path, &ids);
    }
    let _ = db.touch_memories(&ids).await;
    append_kpi_event(&serde_json::json!({
        "ts": now_rfc3339(),
        "event": "recall",
        "session_id": session,
        "tool": payload.tool_name,
        "memories": ids.iter().copied().map(saispace_core::short_id).collect::<Vec<_>>(),
    }));
}
