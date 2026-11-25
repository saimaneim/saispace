//! One convention for memory `topic_key`s, so the same topic written by
//! different agents maps to one row instead of duplicating.
//!
//! A normalized key is lower-case `kebab-case` (`ui-design-rules`, never
//! `ui_design rules` or `UI/Design.Rules`), at most
//! [`MAX_NORMALIZED_TOPIC_KEY_CHARS`] characters, and carries no
//! project-name prefix: in project "saispace" the topic is
//! `ui-design-rules`, not `saispace/ui-design-rules`. Legacy keys imported
//! from engram (`engram/14`) named a row id there, not a topic, so they
//! normalize to nothing.

use crate::paths::project_key;

/// Hard cap for a stored `topic_key`: longer keys are cut at a `-` boundary.
pub const MAX_NORMALIZED_TOPIC_KEY_CHARS: usize = 64;

/// Maps a lower-case accented letter to its ASCII base letter.
fn deaccent(c: char) -> Option<char> {
    Some(match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        _ => return None,
    })
}

/// Lower-case, accents stripped, separators (`_`, spaces, `/`, `.`) turned
/// into `-`, anything else dropped.
fn slug_chars(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars().flat_map(char::to_lowercase) {
        let c = deaccent(c).unwrap_or(c);
        if matches!(c, '_' | '/' | '.') || c.is_whitespace() {
            out.push('-');
        } else if c.is_ascii_alphanumeric() || c == '-' {
            out.push(c);
        }
    }
    out
}

/// Collapses runs of `-` into one and trims them from both ends.
fn collapse_dashes(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_dash = false;
    for c in raw.chars() {
        if c == '-' {
            pending_dash = true;
        } else {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c);
        }
    }
    out
}

/// Removes a leading project-name prefix from `key`, however the project
/// name is written: `saispace/ui`, `Saispace UI` and `saispaceui-…` all
/// count as the project "saispace".
fn strip_project_prefix(key: &str, project_name: &str) -> String {
    for prefix in [project_key(project_name), slug_chars(project_name)] {
        if let Some(rest) = key.strip_prefix(&(prefix + "-"))
            && !rest.is_empty()
        {
            return rest.to_string();
        }
    }
    key.to_string()
}

/// Cuts an over-long key at the last `-` inside the first
/// [`MAX_NORMALIZED_TOPIC_KEY_CHARS`] characters instead of mid-word.
fn truncate_at_dash(key: &str) -> String {
    if key.chars().count() <= MAX_NORMALIZED_TOPIC_KEY_CHARS {
        return key.to_string();
    }
    let head: String = key.chars().take(MAX_NORMALIZED_TOPIC_KEY_CHARS).collect();
    match head.rfind('-') {
        Some(pos) if pos > 0 => head[..pos].to_string(),
        _ => head,
    }
}

/// Whether `trimmed` is a legacy engram key: `engram/<row id>`.
fn is_legacy_engram_key(trimmed: &str) -> bool {
    let lowered = trimmed.to_lowercase();
    let Some(rest) = lowered.strip_prefix("engram/") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// Normalizes a raw `topic_key` for `project_name`. An empty result means
/// "no topic": the raw key was empty, only punctuation, or a legacy
/// `engram/<id>` key.
///
/// ```text
/// normalize_topic_key("saispace/UI_Design.Rules", "saispace") == "ui-design-rules"
/// normalize_topic_key("  Café Ño  ", "")                         == "cafe-no"
/// normalize_topic_key("engram/14", "")                           == ""
/// ```
#[must_use]
pub fn normalize_topic_key(raw: &str, project_name: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || is_legacy_engram_key(trimmed) {
        return String::new();
    }
    let slug = collapse_dashes(&slug_chars(trimmed));
    let slug = strip_project_prefix(&slug, project_name);
    truncate_at_dash(&slug)
}

#[cfg(test)]
mod tests {
    use super::normalize_topic_key as norm;

    #[test]
    fn trims_lowercases_and_unifies_separators() {
        assert_eq!(norm("  Foo_Bar / baz.qux  ", ""), "foo-bar-baz-qux");
        assert_eq!(
            norm("SQLite_UUID Blob encoding", ""),
            "sqlite-uuid-blob-encoding"
        );
        assert_eq!(norm("hello, world!", ""), "hello-world");
        assert_eq!(norm("v2 rules", ""), "v2-rules");
    }

    #[test]
    fn strips_accents() {
        assert_eq!(norm("Diseño UI: Ánodo", ""), "diseno-ui-anodo");
        assert_eq!(norm("Café Ño ÜÚ", ""), "cafe-no-uu");
        assert_eq!(norm("Configuración básica", ""), "configuracion-basica");
    }

    #[test]
    fn collapses_and_trims_dashes() {
        assert_eq!(norm("a--b __ c", ""), "a-b-c");
        assert_eq!(norm("-/-", ""), "");
        assert_eq!(norm("--nas--pujades--", ""), "nas-pujades");
    }

    #[test]
    fn removes_leading_project_prefix() {
        assert_eq!(
            norm("saispace/ui-design-rules", "saispace"),
            "ui-design-rules"
        );
        assert_eq!(norm("Saispace UI Rules", "SAISPACE"), "ui-rules");
        assert_eq!(norm("onixgestoria-auth", "onix_gestoria"), "auth");
        assert_eq!(norm("onix-gestoria-auth", "onix_gestoria"), "auth");
    }

    #[test]
    fn keeps_keys_that_are_not_the_project_prefix() {
        assert_eq!(norm("saispace", "saispace"), "saispace");
        assert_eq!(norm("nas-pujades-backup", "saispace"), "nas-pujades-backup");
    }

    #[test]
    fn legacy_engram_keys_have_no_topic() {
        assert_eq!(norm("engram/14", ""), "");
        assert_eq!(norm(" Engram/033 ", ""), "");
        assert_eq!(norm("engram/14a", ""), "engram-14a");
        assert_eq!(norm("engram/", ""), "engram");
    }

    #[test]
    fn empty_or_punctuation_only_keys_have_no_topic() {
        assert_eq!(norm("", "saispace"), "");
        assert_eq!(norm("   ", "x"), "");
        assert_eq!(norm("/./", ""), "");
    }

    #[test]
    fn caps_length_at_a_dash_boundary() {
        let exact = format!("{}a", "ab-".repeat(21));
        assert_eq!(exact.chars().count(), 64);
        assert_eq!(norm(&exact, ""), exact);

        let no_dash = "abcdefghij".repeat(7);
        assert_eq!(norm(&no_dash, "").chars().count(), 64);

        let dashed = "abcdefgh-".repeat(8);
        let cut = norm(&dashed, "");
        assert_eq!(cut.chars().count(), 62);
        assert!(cut.ends_with("abcdefgh"));
    }
}
