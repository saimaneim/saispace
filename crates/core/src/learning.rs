use serde_json::Value;

pub const CORRECTION_REMINDER: &str = "El usuario te está corrigiendo. Al resolverlo, si es una regla duradera, guárdala como `preference` (o actualiza la existente con su `topic_key`). Si fue un error tuyo con causa no obvia, guárdalo como `bugfix` con la causa.";

pub const STOP_REMINDER: &str = "Hubo correcciones o errores de herramientas sin guardar lección en saispace. Si queda una regla duradera guárdala como `preference` (o actualiza con `topic_key`), y si hubo causa no obvia como `bugfix`; si no aporta nada duradero, termina sin guardar.";

const MAX_PROMPT_CHARS: usize = 800;
/// Token emitted at the start of the text and after each sentence break.
const SENTENCE: &str = "\u{1}";
/// User-role transcript entries written by harnesses, not by the user.
const SYNTHETIC_PREFIXES: &[&str] = &[
    "Delegated task",
    "Used MCP tool",
    "[T3 Code",
    "[Request interrupted",
    "This session is being continued",
    "Caveat:",
    "Base directory for this skill",
];
const IGNORED_TAGS: &[&str] = &["pasted_content", "system-reminder"];

const CORRECTION_PHRASES: &[&[&str]] = &[
    &["no", "hagas"],
    &["no", "lo", "hagas"],
    &["no", "me", "hagas"],
    &["no", "me", "lo", "hagas"],
    &["no", "le", "hagas"],
    &["no", "vuelvas", "a"],
    &["no", "uses"],
    &["no", "lo", "uses"],
    &["no", "pongas"],
    &["no", "toques"],
    &["no", "borres"],
    &["no", "cambies"],
    &["no", "modifiques"],
    &["no", "edites"],
    &["no", "agregues"],
    &["no", "anadas"],
    &["no", "metas"],
    &["no", "ejecutes"],
    &["no", "corras"],
    &["te", "dije"],
    &["te", "lo", "dije"],
    &["te", "he", "dicho"],
    // "otra vez" alone is often a plain request ("corre otra vez los tests").
    &["otra", "vez", "lo", "mismo"],
    &["otra", "vez", "igual"],
    &["otra", "vez", "mal"],
    &["otra", "vez", "con", "eso"],
    &["de", "nuevo", "lo", "mismo"],
    &["again", "the", "same"],
    &["esta", "mal"],
    &["esta", "muy", "mal"],
    &["estan", "mal"],
    &["estan", "muy", "mal"],
    &["hiciste", "mal"],
    &["salio", "mal"],
    &["quedo", "mal"],
    &["no", "es", "asi"],
    &["no", "era", "asi"],
    &["asi", "no"],
    &["deja", "de"],
    // Rule words count only when they open a sentence ("Nunca uses unwrap"),
    // not inside one ("explica por qué nunca pasa").
    &[SENTENCE, "nunca"],
    &[SENTENCE, "jamas"],
    &[SENTENCE, "siempre"],
    &["siempre", "en"],
    &["siempre", "que"],
    &["nunca", "uses"],
    &["nunca", "hagas"],
    &["always", "use"],
    &["never", "use"],
    &["por", "que", "hiciste"],
    &["por", "que", "lo", "hiciste"],
    &["por", "que", "me", "hiciste"],
    &["por", "que", "cambiaste"],
    &["por", "que", "borraste"],
    &["por", "que", "tocaste"],
    &["por", "que", "usaste"],
    &["por", "que", "pusiste"],
    &["por", "que", "modificaste"],
    &["por", "que", "editaste"],
    &["por", "que", "quitaste"],
    &["para", "que", "hiciste"],
    &["porque", "hiciste"],
    &["no", "te", "pedi"],
    &["no", "te", "lo", "pedi"],
    &["no", "pedi"],
    &["no", "he", "pedido"],
    &["nadie", "te", "pidio"],
    &["te", "equivocaste"],
    &["equivocado"],
    &["equivocada"],
    &["cometiste", "un", "error"],
    &["eso", "no", "es", "lo", "que"],
    &["no", "es", "lo", "que"],
    &["no", "era", "lo", "que"],
    &["a", "partir", "de", "ahora"],
    &["de", "ahora", "en", "adelante"],
    &["don't"],
    &["dont"],
    &["do", "not"],
    &["i", "told", "you"],
    &["already", "told", "you"],
    &["thats", "wrong"],
    &["that's", "wrong"],
    &["is", "wrong"],
    &["it's", "wrong"],
    &["its", "wrong"],
    &["got", "it", "wrong"],
    &["you're", "wrong"],
    &["youre", "wrong"],
    &["stop", "doing"],
    &["stop", "using"],
    &["stop", "editing"],
    &["stop", "changing"],
    &["stop", "touching"],
    &["stop", "adding"],
    &["stop", "deleting"],
    &["stop", "modifying"],
    &["stop", "making"],
    &[SENTENCE, "never"],
    &[SENTENCE, "always"],
    &["why", "did", "you"],
    &["why", "would", "you"],
    &["why'd", "you"],
    &["whyd", "you"],
    &["why", "didn't", "you"],
    &["why", "didnt", "you"],
    &["i've", "told", "you"],
    &["ive", "told", "you"],
    &["not", "what", "i", "asked"],
    &["wasn't", "what", "i", "asked"],
    &["wasnt", "what", "i", "asked"],
    &["not", "what", "i", "wanted"],
    &["didn't", "ask"],
    &["didnt", "ask"],
    &["did", "not", "ask"],
    &["you", "broke"],
    &["you", "messed", "up"],
    &["from", "now", "on"],
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TranscriptSignals {
    pub corrections: usize,
    pub failed_tools: usize,
    pub saved_after_last_correction: bool,
    pub edits: usize,
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let target = needle.as_bytes();
    if target.is_empty() || haystack.len() < target.len() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(target.len())
        .position(|w| w.eq_ignore_ascii_case(target))
        .filter(|&idx| haystack.is_char_boundary(idx))
}

enum TagSkip {
    Matched(usize),
    UnclosedTag(usize),
    NoCloseBracket,
}

fn match_ignored_tag_end(rest_after_lt: &str, unclosed_mask: u8) -> TagSkip {
    for (idx, &tag) in IGNORED_TAGS.iter().enumerate() {
        if unclosed_mask & (1 << idx) != 0 {
            continue;
        }
        let len = tag.len();
        if rest_after_lt.len() <= len {
            continue;
        }
        let bytes = rest_after_lt.as_bytes();
        if !bytes[..len].eq_ignore_ascii_case(tag.as_bytes()) {
            continue;
        }
        let delim = bytes[len];
        if delim != b'>' && !delim.is_ascii_whitespace() {
            continue;
        }
        let Some(open_close) = rest_after_lt[len..].find('>') else {
            return TagSkip::NoCloseBracket;
        };
        let after_open = len + open_close + 1;
        let close_tag = format!("</{tag}>");
        let Some(close_pos) = find_ascii_ci(&rest_after_lt[after_open..], &close_tag) else {
            return TagSkip::UnclosedTag(idx);
        };
        return TagSkip::Matched(after_open + close_pos + close_tag.len());
    }
    TagSkip::Matched(0)
}

fn strip_ignored_tags(prompt: &str) -> String {
    let mut out = String::with_capacity(prompt.len());
    let mut pos = 0;
    let mut unclosed_mask = 0u8;
    while let Some(rel_lt) = prompt[pos..].find('<') {
        let lt = pos + rel_lt;
        out.push_str(&prompt[pos..lt]);
        match match_ignored_tag_end(&prompt[lt + 1..], unclosed_mask) {
            TagSkip::Matched(0) => {
                out.push('<');
                pos = lt + 1;
            }
            TagSkip::Matched(skip_len) => {
                pos = lt + 1 + skip_len;
            }
            TagSkip::UnclosedTag(idx) => {
                unclosed_mask |= 1 << idx;
                out.push('<');
                pos = lt + 1;
            }
            TagSkip::NoCloseBracket => {
                out.push_str(&prompt[lt..]);
                return out;
            }
        }
    }
    out.push_str(&prompt[pos..]);
    out
}

fn deaccent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

fn tokenize_prefix(text: &str) -> Vec<String> {
    let mut norm = Vec::with_capacity(text.len().min(MAX_PROMPT_CHARS));
    for c in text.trim_start().chars().take(MAX_PROMPT_CHARS) {
        if ('\u{0300}'..='\u{036f}').contains(&c) {
            continue;
        }
        let normalized = match c {
            '’' | '‘' | 'ʼ' => '\'',
            other => other,
        };
        for lc in normalized.to_lowercase() {
            norm.push(deaccent(lc));
        }
    }

    let mut tokens = vec![SENTENCE.to_string()];
    let mut current = String::new();
    for (i, &c) in norm.iter().enumerate() {
        let joiner_between_alnum = matches!(c, '\'' | '-')
            && i > 0
            && i + 1 < norm.len()
            && norm[i - 1].is_alphanumeric()
            && norm[i + 1].is_alphanumeric();
        if c.is_alphanumeric() || c == '_' || joiner_between_alnum {
            current.push(c);
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            if matches!(c, '.' | '!' | '?' | '\n' | ';' | ':' | '¡' | '¿')
                && tokens.last().is_some_and(|t| t != SENTENCE)
            {
                tokens.push(SENTENCE.to_string());
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_correction_stripped(stripped: &str) -> bool {
    let tokens = tokenize_prefix(stripped);
    if tokens.is_empty() {
        return false;
    }
    CORRECTION_PHRASES.iter().any(|phrase| {
        tokens
            .windows(phrase.len())
            .any(|window| window.iter().zip(phrase.iter()).all(|(a, b)| a == b))
    })
}

#[must_use]
pub fn is_correction(prompt: &str) -> bool {
    let stripped = strip_ignored_tags(prompt);
    let trimmed = stripped.trim_start();
    !SYNTHETIC_PREFIXES.iter().any(|p| trimmed.starts_with(p)) && is_correction_stripped(&stripped)
}

fn is_memory_save_tool(name: &str, input: Option<&Value>) -> bool {
    if name.ends_with("save_memory") || name.contains("consolidate_memories") {
        return true;
    }
    if name.eq_ignore_ascii_case("bash")
        && let Some(cmd) = input.and_then(|v| v.get("command")).and_then(Value::as_str)
    {
        return cmd.contains("sai memory save") || cmd.contains("sai memory merge");
    }
    false
}

fn is_synthetic_text(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('<') || SYNTHETIC_PREFIXES.iter().any(|p| trimmed.starts_with(p))
}

/// `is_error` when present; otherwise results whose text starts like an
/// error (some harnesses omit the flag).
fn is_failed_result(block: &Value, entry: &Value) -> bool {
    if let Some(flag) = block.get("is_error").and_then(Value::as_bool) {
        return flag;
    }
    let looks_failed = |text: &str| {
        let t = text.trim_start();
        t.starts_with("Error:") || t.starts_with("<tool_use_error>")
    };
    let content = block.get("content");
    content.and_then(Value::as_str).is_some_and(looks_failed)
        || content
            .and_then(Value::as_array)
            .and_then(|parts| parts.first())
            .and_then(|part| part.get("text"))
            .and_then(Value::as_str)
            .is_some_and(looks_failed)
        || entry
            .get("toolUseResult")
            .and_then(Value::as_str)
            .is_some_and(looks_failed)
}

fn record_user_prompt(cleaned: &str, signals: &mut TranscriptSignals) {
    if !is_synthetic_text(cleaned) && is_correction_stripped(cleaned) {
        signals.corrections += 1;
        signals.saved_after_last_correction = false;
    }
}

fn process_user_entry(entry: &Value, signals: &mut TranscriptSignals) {
    let Some(content) = entry.get("message").and_then(|m| m.get("content")) else {
        return;
    };
    let is_synthetic = entry
        .get("isMeta")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || entry
            .get("isCompactSummary")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    match content {
        Value::String(text) => {
            if !is_synthetic {
                let cleaned = strip_ignored_tags(text);
                record_user_prompt(&cleaned, signals);
            }
        }
        Value::Array(blocks) => {
            let mut texts = Vec::new();
            for block in blocks {
                let block_type = block.get("type").and_then(Value::as_str);
                if block_type == Some("tool_result") && is_failed_result(block, entry) {
                    signals.failed_tools += 1;
                } else if !is_synthetic
                    && block_type == Some("text")
                    && let Some(text) = block.get("text").and_then(Value::as_str)
                {
                    let cleaned = strip_ignored_tags(text);
                    let trimmed = cleaned.trim_start();
                    if !trimmed.is_empty() && !is_synthetic_text(trimmed) {
                        texts.push(cleaned);
                    }
                }
            }
            if !texts.is_empty() {
                record_user_prompt(&texts.join("\n"), signals);
            }
        }
        _ => {}
    }
}

fn process_assistant_entry(entry: &Value, signals: &mut TranscriptSignals) {
    let Some(blocks) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(name) = block.get("name").and_then(Value::as_str) else {
            continue;
        };
        if is_memory_save_tool(name, block.get("input")) {
            signals.saved_after_last_correction = true;
        }
        if matches!(name, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") {
            signals.edits += 1;
        }
    }
}

#[must_use]
pub fn analyze_transcript(jsonl: &str) -> TranscriptSignals {
    let mut signals = TranscriptSignals::default();
    for line in jsonl.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        match entry.get("type").and_then(Value::as_str) {
            Some("user") => process_user_entry(&entry, &mut signals),
            Some("assistant") => process_assistant_entry(&entry, &mut signals),
            _ => {}
        }
    }
    signals
}

#[cfg(test)]
mod tests {
    use super::{
        CORRECTION_REMINDER, STOP_REMINDER, TranscriptSignals, analyze_transcript, is_correction,
    };

    #[test]
    fn reminder_constants_fit_budget() {
        assert!(CORRECTION_REMINDER.chars().count() <= 300);
        assert!(STOP_REMINDER.chars().count() <= 300);
        assert!(STOP_REMINDER.contains("si no aporta nada duradero, termina sin guardar"));
    }

    #[test]
    fn detects_spanish_and_english_corrections_case_and_accent_insensitively() {
        for sample in [
            "No hagas eso",
            "TE DIJE que uses bun",
            "ya te dije ayer",
            "Otra vez lo mismo",
            "Eso está mal",
            "eso esta mal!",
            "No es así",
            "no es asi",
            "Deja de tocar ese fichero",
            "No vuelvas a hacer commit",
            "NUNCA uses unwrap",
            "Responde SIEMPRE en español",
            "¿Por qué hiciste eso?",
            "por que hiciste eso",
            "No te pedí que cambiaras eso",
            "no te pedi eso",
            "Te equivocaste de ruta",
            "Eso no es lo que quería",
            "Don't edit that file",
            "don’t do that",
            "I told you before",
            "That is WRONG",
            "Stop doing that please",
            "Never mock the database",
            "Always run clippy first",
            "Why did you delete it?",
            "That's not what I asked",
        ] {
            assert!(is_correction(sample), "expected match for: {sample}");
        }
    }

    #[test]
    fn avoids_noise_and_respects_word_boundaries() {
        for sample in [
            "run it again",
            "Handle wrong passwords correctly",
            "Explain why this never happens",
            "explica por qué nunca pasa",
            "Delegated task node:x completed: no hagas nada",
            "corre otra vez los tests",
            "Please try again later",
            "Implement the login endpoint",
            "Ejecuta cargo test --workspace",
            "Revisa la variable is_wrong y always_inline",
            "Use an always-on toggle",
            "Whenever you have time, check it",
            "Esto es normal y minimal",
            "",
            "   ",
        ] {
            assert!(!is_correction(sample), "unexpected match for: {sample}");
        }
    }

    #[test]
    fn ignores_pasted_content_system_reminders_and_tail_after_800_chars() {
        assert!(!is_correction(
            "<system-reminder>Always check rules and never fail</system-reminder>Hola, mira main.rs"
        ));
        assert!(!is_correction(
            "<pasted_content path=\"log.txt\">Esto está mal, don't touch</pasted_content>Resume esto"
        ));
        assert!(is_correction(
            "<system-reminder>ok</system-reminder>\nOtra vez lo mismo"
        ));
        assert!(is_correction(
            "<pasted_content>log</pasted_content> Por qué hiciste esto"
        ));
        assert!(is_correction(
            "<system-reminder>unclosed tag without end, no hagas caso"
        ));

        let padding = "palabra ".repeat(105);
        assert!(padding.chars().count() > 800);
        let with_tail = format!("{padding}never do that");
        assert!(!is_correction(&with_tail));
    }

    #[test]
    fn analyzes_transcript_signals_across_turns_and_malformed_lines() {
        let lines = [
            "not valid json {{{",
            r#"{"type":"system","message":{"content":"ignore"}}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":"Never do this meta"}}"#,
            r#"{"type":"user","message":{"content":"<command-name>wrong</command-name>"}}"#,
            r#"{"type":"user","message":{"content":"Te dije que uses bun"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{}},{"type":"tool_use","name":"Write","input":{}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true},{"type":"tool_result","is_error":false}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__saispace__save_memory","input":{}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<ide_opened_file>x</ide_opened_file>"},{"type":"text","text":"otra vez lo mismo"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"MultiEdit","input":{}},{"type":"tool_use","name":"NotebookEdit","input":{}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}"#,
        ]
        .join("\n");

        assert_eq!(
            analyze_transcript(&lines),
            TranscriptSignals {
                corrections: 2,
                failed_tools: 3,
                saved_after_last_correction: false,
                edits: 4,
            }
        );
    }

    #[test]
    fn recognizes_all_memory_save_variants_and_zero_correction_saves() {
        let with_bash_save = [
            r#"{"type":"user","message":{"content":"don't do that"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"sai memory save --title t --content c"}}]}}"#,
        ]
        .join("\n");
        assert!(analyze_transcript(&with_bash_save).saved_after_last_correction);

        let with_bash_merge = [
            r#"{"type":"user","message":{"content":"don't do that"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"sai memory merge a b --title t --content -"}}]}}"#,
        ]
        .join("\n");
        assert!(analyze_transcript(&with_bash_merge).saved_after_last_correction);

        let with_consolidate = [
            r#"{"type":"user","message":{"content":"wrong"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__saispace__consolidate_memories","input":{}}]}}"#,
        ]
        .join("\n");
        assert!(analyze_transcript(&with_consolidate).saved_after_last_correction);

        let no_corrections_with_save = [
            r#"{"type":"user","message":{"content":"revisa esto"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true},{"type":"tool_result","is_error":true},{"type":"tool_result","is_error":true}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"save_memory","input":{}}]}}"#,
        ]
        .join("\n");
        let signals = analyze_transcript(&no_corrections_with_save);
        assert_eq!(signals.corrections, 0);
        assert_eq!(signals.failed_tools, 3);
        assert!(signals.saved_after_last_correction);
    }

    #[test]
    fn transcript_strips_ignored_tags_before_angle_filter_and_skips_compact_summaries() {
        let lines = [
            r#"{"type":"user","isCompactSummary":true,"message":{"content":"Always check tests and never ignore errors"}}"#,
            r#"{"type":"user","message":{"content":"<system-reminder>ok</system-reminder>\nOtra vez lo mismo"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<pasted_content>log</pasted_content> No hagas eso"}]}}"#,
        ]
        .join("\n");
        let signals = analyze_transcript(&lines);
        assert_eq!(signals.corrections, 2);

        let repeated_unclosed = format!("{} no hagas", "<system-reminder>".repeat(5000));
        assert!(!is_correction(&repeated_unclosed));
        let prefixed_unclosed = format!("no hagas {}", "<system-reminder>".repeat(5000));
        assert!(is_correction(&prefixed_unclosed));
        assert!(is_correction("why didnt you run tests"));
        assert!(is_correction("ive told you twice"));
    }

    #[test]
    fn harness_messages_and_unflagged_errors_are_classified() {
        let jsonl = [
            r#"{"type":"user","message":{"content":"Delegated task node:x [completed]: no hagas eso"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"Used MCP tool: te dije"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"Error: exit 1"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":[{"type":"text","text":"<tool_use_error>boom</tool_use_error>"}]}]}}"#,
            r#"{"type":"user","toolUseResult":"Error: no such file","message":{"content":[{"type":"tool_result","content":"x"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":false,"content":"Error: but flagged ok"}]}}"#,
        ]
        .join("\n");
        let signals = analyze_transcript(&jsonl);
        assert_eq!(signals.corrections, 0);
        assert_eq!(signals.failed_tools, 3);
    }
}
