use chrono::{SecondsFormat, Utc};
use saispace_core::learning::{
    CORRECTION_REMINDER, STOP_REMINDER, analyze_transcript, is_correction,
};
use saispace_core::{default_db_path, kpi_events_path_for};
use serde::Deserialize;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STDIN_MAX_BYTES: u64 = 4 * 1024 * 1024;
const TRANSCRIPT_TAIL_BYTES: u64 = 4 * 1024 * 1024;
const KPI_EVENTS_MAX_BYTES: u64 = 5 * 1024 * 1024;
const KPI_EVENTS_RETENTION_SECS: u64 = 35 * 86_400;

static ROTATE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
struct HookPayload {
    #[serde(default)]
    hook_event_name: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    transcript_path: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    stop_hook_active: Option<bool>,
}

fn read_payload() -> Option<HookPayload> {
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
    serde_json::from_slice(&buf).ok()
}

fn is_single_normal_component(id: &str) -> bool {
    if id.is_empty() || id.contains(['/', '\\', '\0']) {
        return false;
    }
    let mut components = Path::new(id).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn session_marker_path(session_id: Option<&str>) -> Option<PathBuf> {
    let session_id = session_id
        .map(str::trim)
        .filter(|id| is_single_normal_component(id))?;
    let db_path = default_db_path().ok()?;
    Some(db_path.with_file_name("nudged").join(session_id))
}

fn read_tail(path: &Path, max: u64) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start == 0 {
        return Some(text);
    }
    Some(
        text.split_once('\n')
            .map_or_else(String::new, |(_, rest)| rest.to_string()),
    )
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn is_rotated_kpi_log(name: &str) -> bool {
    if name == "kpi-events.jsonl" {
        return false;
    }
    let has_jsonl_ext = Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"));
    (name.starts_with("kpi-events.") && has_jsonl_ext) || name == "kpi-events.jsonl.1"
}

fn parse_rotated_unix_ms(name: &str) -> Option<u128> {
    let rest = name.strip_prefix("kpi-events.")?.strip_suffix(".jsonl")?;
    rest.split('.').next()?.parse::<u128>().ok()
}

fn unique_rotated_path(log_path: &Path, now_ms: u128) -> PathBuf {
    let pid = std::process::id();
    let seq = ROTATE_SEQ.fetch_add(1, Ordering::Relaxed);
    let first = if seq == 0 {
        log_path.with_file_name(format!("kpi-events.{now_ms}.{pid}.jsonl"))
    } else {
        log_path.with_file_name(format!("kpi-events.{now_ms}.{pid}.{seq}.jsonl"))
    };
    if !first.exists() {
        return first;
    }
    for attempt in 1_u64..1_000 {
        let candidate =
            log_path.with_file_name(format!("kpi-events.{now_ms}.{pid}.{seq}.{attempt}.jsonl"));
        if !candidate.exists() {
            return candidate;
        }
    }
    log_path.with_file_name(format!("kpi-events.{now_ms}.{pid}.{seq}.extra.jsonl"))
}

fn cleanup_old_rotated_logs(dir: &Path, now_sys: SystemTime, now_ms: u128) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let retention = Duration::from_secs(KPI_EVENTS_RETENTION_SECS);
    let retention_ms = u128::from(KPI_EVENTS_RETENTION_SECS) * 1_000;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !is_rotated_kpi_log(&name) {
            continue;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let expired_by_name =
            parse_rotated_unix_ms(&name).is_some_and(|ms| now_ms.saturating_sub(ms) > retention_ms);
        let expired_by_mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|mtime| now_sys.duration_since(mtime).ok())
            .is_some_and(|age| age > retention);
        if expired_by_name || expired_by_mtime {
            let _ = fs::remove_file(path);
        }
    }
}

fn append_kpi_event(event: &serde_json::Value) {
    let Ok(db_path) = default_db_path() else {
        return;
    };
    let log_path = kpi_events_path_for(&db_path);
    let Some(parent) = log_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let now_sys = SystemTime::now();
    let now_ms = now_sys
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    cleanup_old_rotated_logs(parent, now_sys, now_ms);
    if let Ok(meta) = fs::metadata(&log_path)
        && meta.len() > KPI_EVENTS_MAX_BYTES
    {
        let rotated = unique_rotated_path(&log_path, now_ms);
        if !rotated.exists() {
            let _ = fs::rename(&log_path, rotated);
        }
    }
    let Ok(mut line) = serde_json::to_string(event) else {
        return;
    };
    line.push('\n');
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

fn handle_user_prompt(payload: &HookPayload) {
    let Some(prompt) = payload.prompt.as_deref() else {
        return;
    };
    if is_correction(prompt) {
        crate::out!("{CORRECTION_REMINDER}");
        let mut event = serde_json::json!({
            "ts": now_rfc3339(),
            "event": "correction",
        });
        if let Some(session_id) = payload
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            && let Some(obj) = event.as_object_mut()
        {
            obj.insert(
                "session_id".to_string(),
                serde_json::Value::String(session_id.to_string()),
            );
        }
        append_kpi_event(&event);
    }
}

fn handle_stop(payload: &HookPayload) {
    let Some(session_id) = payload
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|id| is_single_normal_component(id))
    else {
        return;
    };
    let Some(marker_path) = session_marker_path(Some(session_id)) else {
        return;
    };
    let Some(transcript_path) = payload.transcript_path.as_deref().filter(|p| !p.is_empty()) else {
        return;
    };
    let Some(jsonl) = read_tail(Path::new(transcript_path), TRANSCRIPT_TAIL_BYTES) else {
        return;
    };
    let signals = analyze_transcript(&jsonl);
    let can_block = !payload.stop_hook_active.unwrap_or(false) && !marker_path.exists();
    let needs_block = !signals.saved_after_last_correction
        && (signals.corrections > 0 || signals.failed_tools >= 3);

    let mut blocked = false;
    if can_block
        && needs_block
        && marker_path
            .parent()
            .is_none_or(|parent| fs::create_dir_all(parent).is_ok())
        && fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker_path)
            .is_ok()
    {
        let output = serde_json::json!({
            "decision": "block",
            "reason": STOP_REMINDER,
        });
        if let Ok(line) = serde_json::to_string(&output) {
            crate::out!("{line}");
            blocked = true;
        }
    }

    let event = serde_json::json!({
        "ts": now_rfc3339(),
        "event": "stop",
        "session_id": session_id,
        "corrections": signals.corrections,
        "failed_tools": signals.failed_tools,
        "saved": signals.saved_after_last_correction,
        "blocked": blocked,
    });
    append_kpi_event(&event);
}

pub fn run() {
    let Some(payload) = read_payload() else {
        return;
    };
    match payload.hook_event_name.as_deref() {
        Some("UserPromptSubmit") => handle_user_prompt(&payload),
        Some("Stop") => handle_stop(&payload),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{is_single_normal_component, read_tail};

    #[test]
    fn tail_starts_at_a_line_boundary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, "first line\nsecond\nthird\n").expect("write");
        assert_eq!(read_tail(&path, 9).as_deref(), Some("third\n"));
        assert_eq!(
            read_tail(&path, 1000).as_deref(),
            Some("first line\nsecond\nthird\n")
        );
    }

    #[test]
    fn rejects_invalid_session_ids() {
        assert!(is_single_normal_component("sess-1_abc"));
        assert!(!is_single_normal_component(""));
        assert!(!is_single_normal_component("."));
        assert!(!is_single_normal_component(".."));
        assert!(!is_single_normal_component("../escape"));
        assert!(!is_single_normal_component("a/b"));
        assert!(!is_single_normal_component("a\\b"));
    }
}
