//! Redacts credentials before text reaches the `memories` table (and its FTS
//! index). Known token shapes are always redacted; `NAME=value` is redacted
//! when `NAME` is a secret stored in the vault or looks like a credential.

use regex::Regex;
use std::sync::LazyLock;

pub const REDACTED: &str = "[REDACTADO]";

static TOKEN_SHAPES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        r"|\b(?:sk-ant-|sk-proj-|sk-)[A-Za-z0-9_\-]{20,}",
        r"|\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}|\bgithub_pat_[A-Za-z0-9_]{40,}",
        r"|\bglpat-[A-Za-z0-9_\-]{20,}",
        r"|\bxox[abprs]-[A-Za-z0-9-]{10,}",
        r"|\bAKIA[0-9A-Z]{16}\b",
        r"|\bAIza[0-9A-Za-z_\-]{35}",
        r"|\b(?:sk|rk)_live_[0-9A-Za-z]{20,}",
        r"|\beyJ[A-Za-z0-9_\-]{10,}\.eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        r"|\b[a-z][a-z0-9+.\-]*://[^\s:/@]+:[^\s@/]{3,}@",
    ))
    .expect("valid token regex")
});

/// `KEY=value`, `KEY: value` or `"KEY": "value"`; group 1 is the key, group 2
/// the value.
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b([A-Z][A-Z0-9_]*(?:PASSWORD|PASSWD|SECRET|TOKEN|API_?KEY|PRIVATE_KEY)|[A-Z][A-Z0-9_]{2,})"?\s*[:=]\s*"?([^\s"'`,;]{8,})"#)
        .expect("valid assignment regex")
});

static CREDENTIAL_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(PASSWORD|PASSWD|SECRET|TOKEN|API_?KEY|PRIVATE_KEY|ACCESS_KEY)$")
        .expect("valid name regex")
});

/// Placeholders people write in docs; never worth redacting.
fn is_placeholder(value: &str) -> bool {
    let v = value.trim_matches(|c| c == '<' || c == '>' || c == '$' || c == '{' || c == '}');
    v.chars()
        .all(|c| c == '*' || c == 'x' || c == 'X' || c == '.')
        || v.starts_with("sai secret")
        || value.starts_with('$')
        || value.starts_with('<')
}

/// Returns `text` with credentials replaced by [`REDACTED`] and how many were
/// replaced. `vault_names` are the names of secrets kept in the vault.
#[must_use]
pub fn scrub(text: &str, vault_names: &[String]) -> (String, usize) {
    let mut count = 0;
    let shaped = TOKEN_SHAPES.replace_all(text, |caps: &regex::Captures| {
        count += 1;
        let m = &caps[0];
        // Keep the scheme and user of a URL so the note still makes sense.
        if let Some(at) = m.find("://") {
            let user_end = m[at + 3..].find(':').map_or(m.len(), |i| at + 3 + i);
            return format!("{}:{REDACTED}@", &m[..user_end]);
        }
        REDACTED.to_string()
    });
    let assigned = ASSIGNMENT.replace_all(&shaped, |caps: &regex::Captures| {
        let key = &caps[1];
        let value = &caps[2];
        let sensitive = CREDENTIAL_NAME.is_match(key)
            || vault_names.iter().any(|n| n.eq_ignore_ascii_case(key));
        if !sensitive || is_placeholder(value) || value.contains(REDACTED) {
            return caps[0].to_string();
        }
        count += 1;
        caps[0].replacen(value, REDACTED, 1)
    });
    (assigned.into_owned(), count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_shapes_and_named_values() {
        let text = "token ghp_abcdefghijklmnopqrstuvwxyz0123456789 y \
                    OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwx \
                    DEPLOY_HOOK=https://x.example/abcdef123 \
                    url postgres://app:hunter2secret@db:5432/app";
        let (out, n) = scrub(text, &["DEPLOY_HOOK".to_string()]);
        assert!(!out.contains("ghp_abc"), "{out}");
        assert!(!out.contains("sk-proj"), "{out}");
        assert!(!out.contains("x.example"), "{out}");
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("postgres://app:[REDACTADO]@db"), "{out}");
        assert_eq!(n, 4, "{out}");
    }

    #[test]
    fn leaves_ordinary_notes_and_placeholders_alone() {
        for text in [
            "Usa WAL: PRAGMA journal_mode=WAL para varios lectores",
            "API_KEY=<tu clave> o API_KEY=$OPENAI_API_KEY",
            "la clave se lee con `sai secret run OPENAI_API_KEY -- cmd`",
            "TIMEOUT=30000 y RETRIES=5",
            "PASSWORD=********",
        ] {
            assert_eq!(scrub(text, &[]), (text.to_string(), 0), "{text}");
        }
    }
}
