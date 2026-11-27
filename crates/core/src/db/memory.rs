use crate::db::Db;
use crate::db::search::STOPWORDS;
use crate::db::validation::text_with_limit;
use crate::error::{CoreError, CoreResult};
use crate::models::{
    ConsolidateHint, Memory, MemoryContext, MemoryKind, MemoryScope, SaveMemoryInput, SearchKind,
    SimilarMemory,
};
use crate::topic_key::normalize_topic_key;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use uuid::Uuid;

pub const MAX_MEMORY_TITLE_CHARS: usize = 200;
pub const MAX_MEMORY_CONTENT_CHARS: usize = 20_000;
/// Duplicate pairs listed at session start: enough to prompt a merge,
/// few enough not to cost much.
pub(crate) const MAX_CONSOLIDATE_HINTS: usize = 3;

pub(crate) const MEMORY_COLUMNS: &str = "id, project_id, NULLIF(project_name, '') AS project_name, kind, title, content, NULLIF(topic_key, '') AS topic_key, pinned, created_at, updated_at, last_used_at, use_count, superseded_by";

/// Share of meaningful title words two memories must have in common (Jaccard)
/// to count as possible duplicates. One shared word such as a client name
/// ("Onix", "Pujades") is not enough.
pub(crate) const MIN_TITLE_SIMILARITY: f64 = 0.5;
pub(crate) const BORDERLINE_TITLE_SIMILARITY: f64 = 0.3;
pub(crate) const MIN_CONTENT_OVERLAP: f64 = 0.25;
pub(crate) const MIN_SHARED_CONTENT_WORDS: usize = 4;

const EXTRA_STOPWORDS: &[&str] = &[
    "an", "ante", "are", "as", "at", "bajo", "be", "by", "contra", "cuando", "desde", "donde",
    "durante", "entre", "esa", "esas", "ese", "esos", "esta", "estas", "este", "estos", "from",
    "fue", "han", "hasta", "into", "it", "its", "mas", "mediante", "over", "segun", "sobre",
    "that", "these", "this", "those", "through", "tras", "under", "via", "was", "were",
];

fn fold_char(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

fn fold_accents_lowercase(word: &str) -> String {
    word.chars()
        .flat_map(char::to_lowercase)
        .map(fold_char)
        .collect()
}

fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word) || EXTRA_STOPWORDS.contains(&word)
}

fn is_vowel(b: u8) -> bool {
    matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')
}

fn has_vowel(s: &str) -> bool {
    s.bytes().any(is_vowel)
}

fn vc_measure(s: &str) -> usize {
    let mut measure = 0_usize;
    let mut prev_vowel = false;
    for b in s.bytes() {
        let vowel = is_vowel(b);
        if !vowel && prev_vowel {
            measure += 1;
        }
        prev_vowel = vowel;
    }
    measure
}

fn collapse_double_consonant(s: &mut String) {
    let bytes = s.as_bytes();
    if bytes.len() >= 4 {
        let last = bytes[bytes.len() - 1];
        if last == bytes[bytes.len() - 2]
            && matches!(
                last,
                b'b' | b'd' | b'f' | b'g' | b'm' | b'n' | b'p' | b'r' | b't'
            )
        {
            s.pop();
        }
    }
}

fn strip_plural(w: &str) -> String {
    if w.len() >= 5
        && let Some(stem) = w.strip_suffix("ies")
    {
        return format!("{stem}y");
    }
    if w.len() >= 5
        && (w.ends_with("ches")
            || w.ends_with("shes")
            || w.ends_with("xes")
            || w.ends_with("zes")
            || w.ends_with("sses"))
    {
        return w[..w.len() - 2].to_string();
    }
    if w.len() >= 6 && (w.ends_with("ciones") || w.ends_with("siones")) {
        return w[..w.len() - 2].to_string();
    }
    // No "-ores/-ales/-ades" shortcut: it jumped straight from "sales" to
    // "sal" and "grades" to "grad", merging unrelated common words. The
    // plain "-s" rule below plus final-e stripping already unifies real
    // plurals ("errores" -> "error") without those collisions.
    if w.len() >= 4
        && let Some(stem) = w.strip_suffix('s')
        && !stem.ends_with(['s', 'u', 'i'])
        && !(w.len() == 4 && stem.ends_with('r'))
    {
        return stem.to_string();
    }
    w.to_string()
}

fn stem_token(folded: &str) -> String {
    if !folded.is_ascii() || folded.len() <= 3 {
        return folded.to_string();
    }
    let mut w = strip_plural(folded);

    // Nominalization suffixes. ES/EN cognates must land on one stem:
    // "configuration"/"configuración", "rotation"/"rotación",
    // "session"/"sesión", "version"/"versión".
    if w.len() >= 11
        && let Some(stem) = w
            .strip_suffix("ificacion")
            .or_else(|| w.strip_suffix("ification"))
    {
        w = format!("{stem}if");
    } else if w.len() >= 8
        && let Some(stem) = w
            .strip_suffix("acion")
            .or_else(|| w.strip_suffix("ation"))
            .or_else(|| w.strip_suffix("icion"))
            .or_else(|| w.strip_suffix("ition"))
            .or_else(|| w.strip_suffix("ucion"))
            .or_else(|| w.strip_suffix("ution"))
        && stem.len() >= 4
    {
        w = stem.to_string();
    } else if w.len() >= 6
        && let Some(stem) = w.strip_suffix("ccion")
    {
        w = format!("{stem}ction");
    } else if w.len() >= 5
        && let Some(stem) = w.strip_suffix("cion")
    {
        w = format!("{stem}tion");
    } else if w.len() >= 6
        && let Some(stem) = w.strip_suffix("ion")
        && stem.ends_with(['s', 'x'])
    {
        // "-sión"/"-xión" cognates: "session"/"sesión", "mission"/"misión",
        // "expression"/"expresión". Only after s/x, so "billion", "opinion"
        // or "champion" keep their final syllable. English doubles the "s"
        // ("session" -> "sess") while Spanish does not ("sesión" -> "ses"),
        // so one "s" is folded away here — and only here, to keep pairs
        // like "process"/"processed" intact.
        let mut cand = stem.to_string();
        collapse_double_consonant(&mut cand);
        if cand.ends_with("ss") {
            cand.pop();
        }
        if cand.len() >= 3 {
            w = cand;
        }
    }

    // Gerunds and participles. The stem must stay at least four characters,
    // so "settings" never collapses onto the unrelated common word "set".
    if w.len() >= 6
        && let Some(stem) = w.strip_suffix("ing")
        && has_vowel(stem)
    {
        let mut cand = stem.to_string();
        collapse_double_consonant(&mut cand);
        if cand.len() >= 4 {
            w = cand;
        }
    } else if w.len() >= 5
        && let Some(stem) = w.strip_suffix("ied")
    {
        w = format!("{stem}y");
    } else if w.len() >= 5
        && !w.ends_with("eed")
        && let Some(stem) = w.strip_suffix("ed")
        && has_vowel(stem)
    {
        let mut cand = stem.to_string();
        collapse_double_consonant(&mut cand);
        if cand.len() >= 4 {
            w = cand;
        }
    }

    if w.len() >= 5
        && let Some(stem) = w.strip_suffix("ify")
    {
        w = format!("{stem}if");
    }

    // Final "e" is stripped by the same yardstick for base and inflected
    // forms (one vowel-consonant pair), so "cache"/"cached", "route"/
    // "routing" or "store"/"stored" share one stem instead of diverging.
    // Words of four characters or fewer keep their "e" ("care", "site").
    if w.len() >= 5
        && !w.ends_with("ee")
        && let Some(stem) = w.strip_suffix('e')
        && vc_measure(stem) >= 1
    {
        w = stem.to_string();
    }

    if w.len() >= 4 && w.ends_with('y') && !is_vowel(w.as_bytes()[w.len() - 2]) {
        w.pop();
        w.push('i');
    }

    w
}

pub(crate) fn title_words(title: &str) -> HashSet<String> {
    let mut words = HashSet::new();
    for raw in title.split(|c: char| !c.is_alphanumeric()) {
        if raw.chars().count() < 3 {
            continue;
        }
        let folded = fold_accents_lowercase(raw);
        if is_stopword(&folded) {
            continue;
        }
        let stem = stem_token(&folded);
        if stem.chars().count() >= 2 && !is_stopword(&stem) {
            words.insert(stem);
        }
    }
    words
}

pub(crate) fn memory_tokens(title: &str, content: &str) -> HashSet<String> {
    let mut words = title_words(title);
    for raw in content.split(|c: char| !c.is_alphanumeric()) {
        if raw.chars().count() < 3 {
            continue;
        }
        let folded = fold_accents_lowercase(raw);
        if matches!(folded.as_str(), "what" | "why" | "where" | "learned") || is_stopword(&folded) {
            continue;
        }
        let stem = stem_token(&folded);
        if stem.chars().count() >= 2 && !is_stopword(&stem) {
            words.insert(stem);
        }
    }
    words
}

/// Jaccard similarity of the meaningful normalized words of two titles, from 0.0 to 1.0.
#[must_use]
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (title_words(a), title_words(b));
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = a.intersection(&b).count() as f64 / union as f64;
    ratio
}

impl Db {
    /// # Errors
    ///
    /// Returns an error if the input is invalid or the memory cannot be stored.
    pub async fn save_memory(&self, input: SaveMemoryInput) -> CoreResult<Memory> {
        let memory = self.save_memory_at(input, Utc::now(), None).await?;
        // Connections and housekeeping happen on save, so the agent never
        // has to remember them: references become links, and only the
        // latest session summaries stay active.
        self.link_references(&memory).await?;
        self.archive_old_sessions(&memory).await?;
        Ok(memory)
    }

    /// # Errors
    ///
    /// Returns an error if the input is invalid or the memory cannot be stored.
    pub async fn save_memory_at(
        &self,
        input: SaveMemoryInput,
        updated_at: DateTime<Utc>,
        created_at: Option<DateTime<Utc>>,
    ) -> CoreResult<Memory> {
        let title = text_with_limit("title", &input.title, MAX_MEMORY_TITLE_CHARS)?;
        let content = text_with_limit("content", &input.content, MAX_MEMORY_CONTENT_CHARS)?;
        let project_name = match (input.project_id, input.project_name.as_deref()) {
            (Some(id), _) => {
                self.get_project(id)
                    .await?
                    .ok_or(CoreError::NotFound {
                        entity: "project",
                        id,
                    })?
                    .name
            }
            (None, Some(name)) => name.trim().to_string(),
            (None, None) => String::new(),
        };
        // Empty result = no topic: the upsert below only matches non-empty keys.
        let topic_key = match input.topic_key.as_deref() {
            None => String::new(),
            Some(raw) => normalize_topic_key(raw, &project_name),
        };

        let sql = format!(
            r"
            INSERT INTO memories (id, project_id, project_name, kind, title, content, topic_key, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT (project_name, topic_key) WHERE topic_key <> ''
            DO UPDATE SET kind = excluded.kind, title = excluded.title, content = excluded.content,
                          project_id = excluded.project_id, updated_at = excluded.updated_at
            RETURNING {MEMORY_COLUMNS}
            "
        );
        let memory = sqlx::query_as::<_, Memory>(&sql)
            .bind(Uuid::now_v7())
            .bind(input.project_id)
            .bind(project_name)
            .bind(input.kind)
            .bind(title)
            .bind(content)
            .bind(topic_key)
            .bind(created_at.unwrap_or(updated_at))
            .bind(updated_at)
            .fetch_one(&self.pool)
            .await?;
        Ok(memory)
    }

    /// # Errors
    ///
    /// Returns an error if the memory does not exist or the query fails.
    pub async fn get_memory(&self, id: Uuid) -> CoreResult<Memory> {
        let sql = format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE id = ?1");
        sqlx::query_as::<_, Memory>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(CoreError::NotFound {
                entity: "memory",
                id,
            })
    }

    /// # Errors
    ///
    /// Returns an error if the memories cannot be loaded.
    pub async fn recent_memories(&self, scope: MemoryScope, limit: i64) -> CoreResult<Vec<Memory>> {
        let (mode, project_id) = match scope {
            MemoryScope::All => (0, None),
            MemoryScope::GlobalOnly => (1, None),
            MemoryScope::Project(id) => (2, Some(id)),
        };
        let sql = format!(
            r"
            SELECT {MEMORY_COLUMNS}
            FROM memories
            WHERE superseded_by IS NULL
              AND (?1 = 0
                   OR (project_id IS NULL AND project_name = '')
                   OR (?1 = 2 AND project_id = ?2))
            ORDER BY updated_at DESC
            LIMIT ?3
            "
        );
        let items = sqlx::query_as::<_, Memory>(&sql)
            .bind(mode)
            .bind(project_id)
            .bind(limit.clamp(1, 200))
            .fetch_all(&self.pool)
            .await?;
        Ok(items)
    }

    /// Session-start context: every preference, pinned memories, the newest
    /// session summary (older ones are superseded) and up to `limit` others.
    ///
    /// # Errors
    ///
    /// Returns an error if the memories cannot be loaded.
    pub async fn memory_context(
        &self,
        scope: MemoryScope,
        limit: i64,
    ) -> CoreResult<MemoryContext> {
        let dupes = self.duplicate_pairs(scope, MAX_CONSOLIDATE_HINTS).await?;
        self.memory_context_with_dupes(scope, limit, &dupes).await
    }

    /// # Errors
    ///
    /// Returns an error if the memories cannot be loaded.
    pub async fn memory_context_with_dupes(
        &self,
        scope: MemoryScope,
        limit: i64,
        dupes: &[crate::db::DuplicatePair],
    ) -> CoreResult<MemoryContext> {
        let limit = usize::try_from(limit.clamp(1, 100)).unwrap_or(1);
        let mut context = MemoryContext::default();
        for memory in self.recent_memories(scope, 200).await? {
            match memory.kind {
                MemoryKind::Preference => context.preferences.push(memory),
                _ if memory.pinned => context.pinned.push(memory),
                MemoryKind::SessionSummary => {
                    if context.last_session.is_none() {
                        context.last_session = Some(memory);
                    }
                }
                _ if context.memories.len() < limit => context.memories.push(memory),
                _ => {}
            }
        }
        context.consolidate = dupes
            .iter()
            .take(MAX_CONSOLIDATE_HINTS)
            .map(|pair| ConsolidateHint {
                a: pair.a.id,
                a_title: pair.a.title.clone(),
                b: pair.b.id,
                b_title: pair.b.title.clone(),
            })
            .collect();
        Ok(context)
    }

    /// Memories in the same scope whose title shares words with `memory`'s
    /// title, so the caller can merge or delete duplicates instead of piling
    /// them up. Session summaries are history, never duplicates.
    ///
    /// # Errors
    ///
    /// Returns an error if the search or the follow-up lookups fail.
    pub async fn similar_memories(
        &self,
        memory: &Memory,
        limit: i64,
    ) -> CoreResult<Vec<SimilarMemory>> {
        if memory.kind == MemoryKind::SessionSummary {
            return Ok(Vec::new());
        }
        let scope = MemoryScope::for_project(memory.project_id);
        let hits = self
            .search_titles(&memory.title, scope, limit.saturating_add(10))
            .await?;
        let mut similar = Vec::new();
        for hit in hits {
            if hit.kind != SearchKind::Memory
                || hit.id == memory.id
                || title_similarity(&memory.title, &hit.title) < MIN_TITLE_SIMILARITY
            {
                continue;
            }
            let other = self.get_memory(hit.id).await?;
            if other.kind == MemoryKind::SessionSummary {
                continue;
            }
            similar.push(SimilarMemory {
                topic_key: other.topic_key.clone().unwrap_or_default(),
                hit,
            });
            if similar.len() >= usize::try_from(limit).unwrap_or(0) {
                break;
            }
        }
        Ok(similar)
    }

    /// # Errors
    ///
    /// Returns an error if the memory does not exist or the delete fails.
    pub async fn delete_memory(&self, id: Uuid) -> CoreResult<()> {
        let result = sqlx::query("DELETE FROM memories WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound {
                entity: "memory",
                id,
            });
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the memories cannot be updated.
    pub async fn link_memories_by_name(&self, project_id: Uuid, name: &str) -> CoreResult<u64> {
        let result = sqlx::query(
            "UPDATE memories SET project_id = ?1 WHERE project_id IS NULL AND project_name = ?2",
        )
        .bind(project_id)
        .bind(name)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::title_similarity;

    #[test]
    fn title_similarity_ignores_a_single_shared_name() {
        assert!(
            title_similarity(
                "Onix: encargos duplicados por carrera con la IA",
                "Onix: trabajador 1 de Cegid empresa 552 sobrescrito",
            ) < 0.5
        );
        assert!(
            title_similarity(
                "Stack detection, tech_rules and FTS5 search",
                "saispace UI rules and AI context files",
            ) < 0.5
        );
        assert!(
            title_similarity(
                "Database connection pooling configuration",
                "Database connection pooling settings",
            ) >= 0.5
        );
        assert!(title_similarity("Baúl de claves", "Claves del baúl cifradas") >= 0.5);
        assert!(title_similarity("", "de la") < 0.5);
    }

    #[test]
    fn title_similarity_normalizes_plurals_stems_accents_and_cognates() {
        assert!(
            title_similarity(
                "PostgreSQL connection pool configuration",
                "PostgreSQL connection pooling settings",
            ) >= 0.5
        );
        assert!(
            title_similarity(
                "Stripe webhook signature verification",
                "Verify Stripe webhook HMAC signatures",
            ) >= 0.5
        );
        assert!(
            title_similarity(
                "Idempotency keys for payment charges",
                "Payment charge idempotency key handling",
            ) >= 0.5
        );
        assert!(
            title_similarity(
                "Configuración de rotación de claves",
                "Configuration key rotation",
            ) >= 0.5
        );
        assert!(
            title_similarity(
                "Retry failed webhook deliveries",
                "Retried failed webhook delivery",
            ) >= 0.5
        );
        assert!(title_similarity("CORS origins allowed", "Core engine limits",) < 0.5);
    }

    #[test]
    fn title_similarity_rejects_false_friend_stems() {
        let distinct = [
            // "-ing" must not collapse "settings" onto "set".
            ("Settings", "Set de datos"),
            // The plural rules must not merge unrelated words.
            ("Sales report", "Salt water"),
            ("Grades validation", "Grad student rules"),
            ("Locales comerciales", "Local development"),
            // Consistent e-stripping must not create new collisions.
            ("Cache invalidation", "Cash flow report"),
            ("Static analysis rules", "Stats collection rules"),
            ("Session handling", "SES email setup"),
            ("Care package", "Car rental"),
            ("Route limits", "Root causes"),
            ("Changed files", "Charge files"),
            // "-ion" stripping stays away from look-alikes.
            ("Billing totals", "Billion events"),
            ("Question answering", "Quest rewards"),
            ("Mission planning", "Miss meeting"),
            ("Production deploy", "Product catalog"),
            // Different topics sharing a single word are never duplicates.
            ("Connection pool", "Content pool"),
            ("Keys rotation", "Key performance"),
            ("Site reliability", "Sitemap generation"),
        ];
        for (a, b) in distinct {
            assert!(
                title_similarity(a, b) < 0.5,
                "false friends matched: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn title_similarity_matches_inflections_plurals_and_cognates() {
        let same_topic = [
            // Base and inflected forms now share one stem.
            ("Cache eviction policy", "Cached eviction policy"),
            ("Route matching rules", "Routing matching rules"),
            ("Score aggregation limits", "Scoring aggregation limits"),
            ("Share link expiry", "Sharing link expiry"),
            ("Store credentials safely", "Stored credentials safely"),
            ("Merge conflicts handling", "Merged conflicts handling"),
            ("Change password process", "Changed password process"),
            ("Streaming exports", "Streamed exports"),
            ("Copied files", "Copy files"),
            // ES/EN cognates converge through the nominalization suffixes.
            ("Session tokens", "Tokens de sesión"),
            ("Control de versiones", "Version control"),
            ("Expresión regular", "Regular expression"),
            ("Decisión final", "Final decision"),
            ("Extensión de browser", "Browser extension"),
            ("Errores de validación", "Validation errors"),
            // Plurals on both languages.
            ("Policy enforcement", "Policies enforcement"),
            ("Claves de acceso", "Clave de acceso"),
            ("Origins allowlist", "Origin allowlist"),
        ];
        for (a, b) in same_topic {
            assert!(
                title_similarity(a, b) >= 0.5,
                "expected duplicates failed to match: {a:?} vs {b:?}"
            );
        }
    }
}
