use saispace_core::default_db_path;
use saispace_core::learning::{
    CORRECTION_REMINDER, STOP_REMINDER, analyze_transcript, is_correction,
};
use serde::Deserialize;
use std::fs;
use std::io::{IsTerminal, Read};
use std::path::{Component, Path, PathBuf};

const STDIN_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Stop runs after every turn: read only the recent end of long transcripts
/// (a 30 MB one took over a second in full). Older corrections already got
/// their reminder when the prompt was sent.
const TRANSCRIPT_TAIL_BYTES: u64 = 4 * 1024 * 1024;

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

/// The last `max` bytes of a file, starting at a line boundary.
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
    // Drop the partial first line.
    Some(
        text.split_once('\n')
            .map_or_else(String::new, |(_, rest)| rest.to_string()),
    )
}

fn handle_user_prompt(prompt: Option<&str>) {
    let Some(prompt) = prompt else {
        return;
    };
    if is_correction(prompt) {
        crate::out!("{CORRECTION_REMINDER}");
    }
}

fn handle_stop(payload: &HookPayload) {
    if payload.stop_hook_active.unwrap_or(false) {
        return;
    }
    let Some(marker_path) = session_marker_path(payload.session_id.as_deref()) else {
        return;
    };
    if marker_path.exists() {
        return;
    }
    let Some(transcript_path) = payload.transcript_path.as_deref().filter(|p| !p.is_empty()) else {
        return;
    };
    let Some(jsonl) = read_tail(Path::new(transcript_path), TRANSCRIPT_TAIL_BYTES) else {
        return;
    };
    let signals = analyze_transcript(&jsonl);
    if signals.saved_after_last_correction || (signals.corrections == 0 && signals.failed_tools < 3)
    {
        return;
    }
    if let Some(parent) = marker_path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    if fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker_path)
        .is_err()
    {
        return;
    }
    let output = serde_json::json!({
        "decision": "block",
        "reason": STOP_REMINDER,
    });
    if let Ok(line) = serde_json::to_string(&output) {
        crate::out!("{line}");
    }
}

pub fn run() {
    let Some(payload) = read_payload() else {
        return;
    };
    match payload.hook_event_name.as_deref() {
        Some("UserPromptSubmit") => handle_user_prompt(payload.prompt.as_deref()),
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
