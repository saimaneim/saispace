//! Lessons that come back on their own: a memory can carry triggers (an error
//! text, a command fragment, or `env:<host>`), so when the same failure shows
//! up again the agent gets the fix before retrying, and when it runs inside a
//! given host (T3 Code, `OpenCode`…) it starts with that host's lessons.

use crate::memory_render::excerpt;
use crate::models::Memory;
use std::fmt::Write as _;

/// Hosts an `env:` trigger can name.
pub const KNOWN_HOSTS: &[&str] = &["t3code", "claude-code", "opencode", "codex", "gemini"];

/// Triggers per memory: a handful of error signatures is enough.
pub const MAX_TRIGGERS: usize = 8;
/// Shorter patterns match unrelated output (`error`, `failed`).
pub const MIN_TRIGGER_CHARS: usize = 6;
pub const MAX_TRIGGER_CHARS: usize = 160;
/// Recalled lessons per tool call, so one noisy output cannot flood the turn.
pub const MAX_RECALL_HITS: usize = 2;

/// Text that shows up in almost every failure and would recall a lesson for
/// unrelated errors.
const GENERIC_TRIGGERS: &[&str] = &[
    "error",
    "error:",
    "failed",
    "failure",
    "exception",
    "traceback",
    "warning",
    "exit code 1",
    "exit code 2",
    "exit code 127",
    "not found",
    "command not found",
    "no such file or directory",
    "permission denied",
    "timed out",
    "timeout",
    "undefined",
];

#[must_use]
pub fn host_label(host: &str) -> &str {
    match host {
        "t3code" => "T3 Code",
        "claude-code" => "Claude Code",
        "opencode" => "OpenCode",
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        other => other,
    }
}

/// Hosts the current process runs inside, outermost first. `SAISPACE_HOST`
/// (comma separated) adds hosts a wrapper knows about, such as the `OpenCode`
/// plugin, which spawns `sai` from its own process.
#[must_use]
pub fn detect_hosts(var: impl Fn(&str) -> Option<String>) -> Vec<&'static str> {
    let set = |key: &str| var(key).is_some_and(|v| !v.trim().is_empty());
    let mut hosts = Vec::new();
    let mut push = |host: &'static str| {
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    };
    if set("T3CODE_THREAD_ID") || set("T3CODE_CLI_PATH") {
        push("t3code");
    }
    if let Some(extra) = var("SAISPACE_HOST") {
        for name in extra.split(',') {
            let name = name.trim().to_ascii_lowercase();
            if let Some(known) = KNOWN_HOSTS.iter().find(|h| **h == name) {
                push(known);
            }
        }
    }
    if set("CLAUDECODE") {
        push("claude-code");
    }
    if set("OPENCODE") || set("OPENCODE_BIN_PATH") || set("OPENCODE_SESSION_ID") {
        push("opencode");
    }
    if set("CODEX_HOME") || set("CODEX_SANDBOX") {
        push("codex");
    }
    if set("GEMINI_CLI") {
        push("gemini");
    }
    hosts
}

/// Lowercase with runs of whitespace collapsed, so a trigger matches output
/// regardless of line wrapping or indentation.
#[must_use]
pub fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

/// Validates and normalizes one trigger.
///
/// # Errors
///
/// Returns a message for an empty, too short or too long pattern, a pattern so
/// generic it would fire on unrelated failures, or an unknown `env:` host.
pub fn normalize_trigger(raw: &str) -> Result<String, String> {
    let pattern = normalize_text(raw);
    if let Some(host) = pattern.strip_prefix("env:") {
        let host = host.trim();
        return if KNOWN_HOSTS.contains(&host) {
            Ok(format!("env:{host}"))
        } else {
            Err(format!(
                "host desconocido en '{raw}': usa env:{}",
                KNOWN_HOSTS.join(", env:")
            ))
        };
    }
    if GENERIC_TRIGGERS.contains(&pattern.trim_end_matches([':', '.']).trim()) {
        return Err(format!(
            "disparador '{raw}' demasiado genérico: usa el texto propio del error (p. ej. 'permission denied (publickey)')"
        ));
    }
    let chars = pattern.chars().count();
    if chars < MIN_TRIGGER_CHARS {
        return Err(format!(
            "disparador '{raw}' demasiado corto (mínimo {MIN_TRIGGER_CHARS} caracteres)"
        ));
    }
    if chars > MAX_TRIGGER_CHARS {
        return Err(format!(
            "disparador demasiado largo (máximo {MAX_TRIGGER_CHARS} caracteres): usa la línea clave del error"
        ));
    }
    Ok(pattern)
}

/// Normalizes, dedupes and caps a trigger list.
///
/// # Errors
///
/// Returns the first invalid trigger's message, or one when there are more than
/// [`MAX_TRIGGERS`].
pub fn normalize_triggers<S: AsRef<str>>(raw: &[S]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let pattern = normalize_trigger(item.as_ref())?;
        if !out.contains(&pattern) {
            out.push(pattern);
        }
    }
    if out.len() > MAX_TRIGGERS {
        return Err(format!("máximo {MAX_TRIGGERS} disparadores por memoria"));
    }
    Ok(out)
}

/// Whether a normalized trigger fires for a normalized haystack (tool input
/// and output) or one of the detected hosts.
#[must_use]
pub fn trigger_fires(pattern: &str, haystack: &str, hosts: &[&str]) -> bool {
    match pattern.strip_prefix("env:") {
        Some(host) => hosts.contains(&host),
        None => haystack.contains(pattern),
    }
}

/// The part of a memory worth repeating when it is recalled: the `Learned:`
/// line when there is one (the fix), otherwise the start of the content.
#[must_use]
pub fn lesson_text(content: &str, max: usize) -> String {
    let flat = excerpt(content, usize::MAX);
    let lesson = [
        "**Learned:**",
        "**Learned**:",
        "Learned:",
        "Lección:",
        "How to apply:",
    ]
    .iter()
    .find_map(|marker| flat.find(marker).map(|i| flat[i + marker.len()..].trim()))
    .filter(|rest| !rest.is_empty())
    .unwrap_or(&flat);
    excerpt(lesson, max)
}

/// Context handed to the agent right after a tool call whose input or output
/// matched a lesson.
#[must_use]
pub fn render_recall(hits: &[Memory]) -> String {
    let mut out = String::from(
        "saispace: esto ya pasó antes. Aplica la lección guardada antes de reintentar o de pedir ayuda al usuario:\n",
    );
    for m in hits {
        let _ = writeln!(
            out,
            "- {} [{}]: {}",
            excerpt(&m.title, 90),
            crate::short_id(m.id),
            lesson_text(&m.content, 400)
        );
    }
    out.push_str(
        "Si la lección ya no sirve o estaba mal, corrígela con el mismo topic_key en vez de ignorarla.",
    );
    out
}

/// Short, stable signature of a failed tool's output, suggested as a trigger
/// when the agent saves the fix: the first meaningful line, without volatile
/// numbers such as PIDs, ports or timings.
#[must_use]
pub fn error_signature(output: &str) -> Option<String> {
    let mut lines = output
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches("<tool_use_error>")
                .trim_end_matches("</tool_use_error>")
                .trim()
        })
        .filter(|l| l.chars().filter(|c| c.is_alphabetic()).count() >= 4);
    let first = lines.next()?;
    // "Exit code N" says little on its own: prefer the message under it, and
    // keep the code only when it is distinctive (144 = killed by a signal).
    if let Some(code) = first
        .strip_prefix("Exit code ")
        .filter(|c| c.chars().all(|ch| ch.is_ascii_digit()))
    {
        return lines
            .find_map(signature_of_line)
            .or_else(|| (!matches!(code, "1" | "2" | "127")).then(|| format!("exit code {code}")));
    }
    signature_of_line(first)
}

/// Contiguous run of stable words: leading volatile tokens (IPs, paths,
/// `user@host`) are skipped, and the run stops at the next one, so the result
/// stays a substring of future occurrences of the same error.
fn signature_of_line(line: &str) -> Option<String> {
    let volatile = |word: &str| {
        word.chars().filter(char::is_ascii_digit).count() >= 3
            || word.contains('@')
            || word.starts_with('/')
            || word.starts_with("~/")
    };
    let mut sig = String::new();
    for word in line.split_whitespace().skip_while(|w| volatile(w)) {
        if volatile(word) {
            // A code after a short prefix ("MCP error -32602: items…") says
            // less than the message after it: start over from there.
            if sig.split(' ').count() <= 2 {
                sig.clear();
                continue;
            }
            break;
        }
        if sig.chars().count() + word.chars().count() > 80 {
            break;
        }
        if !sig.is_empty() {
            sig.push(' ');
        }
        sig.push_str(word);
    }
    normalize_trigger(sig.trim_end_matches([':', ',', '.', ';'])).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |key: &str| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn detects_nested_hosts() {
        let hosts = detect_hosts(env(&[("T3CODE_THREAD_ID", "abc"), ("CLAUDECODE", "1")]));
        assert_eq!(hosts, vec!["t3code", "claude-code"]);
        let hosts = detect_hosts(env(&[("SAISPACE_HOST", "OpenCode, bogus")]));
        assert_eq!(hosts, vec!["opencode"]);
        assert_eq!(
            detect_hosts(env(&[("T3CODE_THREAD_ID", "  ")])),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn normalizes_and_rejects_triggers() {
        assert_eq!(
            normalize_trigger("  Permission denied   (publickey) ").as_deref(),
            Ok("permission denied (publickey)")
        );
        assert_eq!(
            normalize_trigger("ENV: T3Code").as_deref(),
            Ok("env:t3code")
        );
        assert!(normalize_trigger("env:vscode").is_err());
        assert!(normalize_trigger("error:").is_err());
        assert!(normalize_trigger("Permission denied").is_err());
        assert!(normalize_trigger("abc").is_err());
        assert!(normalize_trigger(&"x".repeat(200)).is_err());
        let list = normalize_triggers(&["Exit code 144", "exit  code 144", "env:t3code"]);
        assert_eq!(
            list,
            Ok(vec!["exit code 144".to_string(), "env:t3code".to_string()])
        );
        let many: Vec<String> = (0..9).map(|i| format!("signature {i}")).collect();
        assert!(normalize_triggers(&many).is_err());
    }

    #[test]
    fn fires_on_output_or_host() {
        let hay = normalize_text("ssh: connect\nroot@1.2.3.4: Permission denied (publickey).");
        assert!(trigger_fires("permission denied (publickey)", &hay, &[]));
        assert!(!trigger_fires("exit code 144", &hay, &[]));
        assert!(trigger_fires("env:t3code", "", &["t3code"]));
        assert!(!trigger_fires("env:t3code", "env:t3code", &[]));
    }

    #[test]
    fn lesson_prefers_learned_line() {
        let content = "What: falló ssh-add.\nWhy: sin TTY.\nLearned: pide al usuario que lo ejecute en su terminal.";
        assert_eq!(
            lesson_text(content, 200),
            "pide al usuario que lo ejecute en su terminal."
        );
        assert_eq!(lesson_text("solo texto", 200), "solo texto");
        assert_eq!(lesson_text("What: x\n**Learned:** usa y", 200), "usa y");
    }

    #[test]
    fn signature_drops_volatile_parts() {
        assert_eq!(
            error_signature("root@2.57.90.104: Permission denied (publickey).").as_deref(),
            Some("permission denied (publickey)")
        );
        assert_eq!(
            error_signature("Exit code 144").as_deref(),
            Some("exit code 144")
        );
        assert_eq!(
            error_signature("MCP error -32602: items also needs \"scope\"").as_deref(),
            Some("items also needs \"scope\"")
        );
        assert_eq!(
            error_signature("Exit code 1\nfatal: not a git repository (or any parent)").as_deref(),
            Some("fatal: not a git repository (or any parent)")
        );
        assert_eq!(
            error_signature("Could not open a connection to your authentication agent.").as_deref(),
            Some("could not open a connection to your authentication agent")
        );
        assert_eq!(
            error_signature("<tool_use_error>Blocked: sleep 60 followed by tail</tool_use_error>")
                .as_deref(),
            Some("blocked: sleep 60 followed by tail")
        );
        assert_eq!(
            error_signature("zsh: no matches found: --include=*.ts").as_deref(),
            Some("zsh: no matches found: --include=*.ts")
        );
        assert_eq!(error_signature("Exit code 1").as_deref(), None);
        assert_eq!(error_signature("   \n").as_deref(), None);
    }
}
