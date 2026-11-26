//! Connections and consolidation: links between memories, short ids that are
//! cheap to print, and merging several memories into one without losing them.

use crate::db::Db;
use crate::db::memory::{MAX_MEMORY_CONTENT_CHARS, MAX_MEMORY_TITLE_CHARS};
use crate::db::validation::text_with_limit;
use crate::error::{CoreError, CoreResult};
use crate::models::{
    ConsolidateInput, LinkDirection, Memory, MemoryKind, MemoryLink, MemoryRelation,
};
use crate::topic_key::normalize_topic_key;
use chrono::Utc;
use std::collections::HashSet;
use uuid::Uuid;

/// Session summaries that stay active per project; older ones are archived
/// (still readable by id, out of the index and of search).
pub const KEEP_SESSIONS: usize = 3;
const MAX_CONSOLIDATE: usize = 20;
const SHORT_ID_LEN: usize = 8;
/// Topic keys shorter than this, or without a dash, are too generic to
/// count as a reference when they appear in another memory's text.
const MIN_TOPIC_REF_LEN: usize = 6;

/// Last 8 hex digits of the id: the random tail of a `UUIDv7`, so unique in
/// practice. Prints 28 characters fewer than the full id in every listing.
#[must_use]
pub fn short_id(id: Uuid) -> String {
    let simple = id.simple().to_string();
    simple[simple.len() - SHORT_ID_LEN..].to_string()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// True when `needle` appears in `haystack` as a whole token.
fn contains_token(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(start, _)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + needle.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

/// Full UUIDs and 8-digit hex short ids mentioned in `text`.
fn id_mentions(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .filter_map(|token| {
            let token = token.trim_matches('-');
            if Uuid::parse_str(token).is_ok() {
                return Some(token.replace('-', "").to_ascii_lowercase());
            }
            // Any 8-digit hex token may be a short id (some are all digits
            // or all letters); it only becomes a link if a memory id ends
            // with it, so dates or words like "deadbeef" link nothing.
            (token.len() == SHORT_ID_LEN).then(|| token.to_ascii_lowercase())
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    id: Uuid,
    title: String,
    kind: MemoryKind,
    relation: MemoryRelation,
    outgoing: bool,
    archived: bool,
}

#[derive(sqlx::FromRow)]
struct TopicRow {
    id: Uuid,
    topic_key: String,
}

impl Db {
    /// Accepts a full UUID or a unique hex suffix (the short id printed in
    /// listings, at least 6 digits).
    ///
    /// # Errors
    ///
    /// Returns an error if nothing matches, or a short id matches several.
    pub async fn resolve_memory_id(&self, raw: &str) -> CoreResult<Uuid> {
        let raw = raw.trim();
        if let Ok(id) = Uuid::parse_str(raw) {
            return Ok(id);
        }
        let hex: String = raw.chars().filter(|c| *c != '-').collect();
        if !(6..=32).contains(&hex.len()) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CoreError::Validation(format!(
                "'{raw}' is not a memory id: pass the full id or its last 8 hex digits"
            )));
        }
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM memories WHERE lower(hex(id)) LIKE '%' || ?1 LIMIT 3",
        )
        .bind(hex.to_ascii_lowercase())
        .fetch_all(&self.pool)
        .await?;
        match ids.as_slice() {
            [id] => Ok(*id),
            [] => Err(CoreError::Validation(format!(
                "no memory with id ending in {hex}"
            ))),
            _ => Err(CoreError::Validation(format!(
                "id {hex} matches several memories: use more digits"
            ))),
        }
    }

    /// Links `from` to `to`. Returns false when the link already existed.
    ///
    /// # Errors
    ///
    /// Returns an error if either memory does not exist or both are the same.
    pub async fn link_memories(
        &self,
        from: Uuid,
        to: Uuid,
        relation: MemoryRelation,
    ) -> CoreResult<bool> {
        if from == to {
            return Err(CoreError::Validation(
                "a memory cannot link to itself".to_string(),
            ));
        }
        self.get_memory(from).await?;
        self.get_memory(to).await?;
        let result = sqlx::query(
            "INSERT OR IGNORE INTO memory_links (from_id, to_id, relation, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(from)
        .bind(to)
        .bind(relation)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Removes every link between the two memories, in both directions.
    ///
    /// # Errors
    ///
    /// Returns an error if the delete fails.
    pub async fn unlink_memories(&self, a: Uuid, b: Uuid) -> CoreResult<u64> {
        let result = sqlx::query(
            "DELETE FROM memory_links WHERE (from_id = ?1 AND to_id = ?2) OR (from_id = ?2 AND to_id = ?1)",
        )
        .bind(a)
        .bind(b)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Neighbours of a memory in both directions, newest link first.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn memory_links(&self, id: Uuid) -> CoreResult<Vec<MemoryLink>> {
        let rows = sqlx::query_as::<_, LinkRow>(
            r"
            SELECT m.id, m.title, m.kind, l.relation, 1 AS outgoing,
                   m.superseded_by IS NOT NULL AS archived, l.created_at AS linked_at
            FROM memory_links l JOIN memories m ON m.id = l.to_id
            WHERE l.from_id = ?1
            UNION ALL
            SELECT m.id, m.title, m.kind, l.relation, 0 AS outgoing,
                   m.superseded_by IS NOT NULL AS archived, l.created_at AS linked_at
            FROM memory_links l JOIN memories m ON m.id = l.from_id
            WHERE l.to_id = ?1
            ORDER BY linked_at DESC
            ",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| MemoryLink {
                id: r.id,
                title: r.title,
                kind: r.kind,
                relation: r.relation,
                direction: if r.outgoing {
                    LinkDirection::Out
                } else {
                    LinkDirection::In
                },
                archived: r.archived,
            })
            .collect())
    }

    /// Links `memory` to the memories its text mentions: by id (full or
    /// short) or by `topic_key` in the same scope. Returns how many links
    /// were added. Agents already write "see opencode-plugin-api-v2"; this
    /// turns that into a link they can follow without searching.
    ///
    /// # Errors
    ///
    /// Returns an error if the lookups or inserts fail.
    pub async fn link_references(&self, memory: &Memory) -> CoreResult<usize> {
        let text = format!("{}\n{}", memory.title, memory.content);
        let mut targets: HashSet<Uuid> = HashSet::new();

        for mention in id_mentions(&text) {
            let ids: Vec<Uuid> = sqlx::query_scalar(
                "SELECT id FROM memories WHERE lower(hex(id)) LIKE '%' || ?1 LIMIT 2",
            )
            .bind(&mention)
            .fetch_all(&self.pool)
            .await?;
            if let [id] = ids.as_slice() {
                targets.insert(*id);
            }
        }

        let topics = sqlx::query_as::<_, TopicRow>(
            r"
            SELECT id, topic_key FROM memories
            WHERE topic_key <> '' AND superseded_by IS NULL AND id <> ?1
              AND (project_id IS ?2 OR (project_id IS NULL AND project_name = ''))
            ",
        )
        .bind(memory.id)
        .bind(memory.project_id)
        .fetch_all(&self.pool)
        .await?;
        let lower = text.to_lowercase();
        for topic in topics {
            let key = topic.topic_key.to_lowercase();
            if key.len() >= MIN_TOPIC_REF_LEN && key.contains('-') && contains_token(&lower, &key) {
                targets.insert(topic.id);
            }
        }

        targets.remove(&memory.id);
        let mut added = 0;
        for target in targets {
            let result = sqlx::query(
                "INSERT OR IGNORE INTO memory_links (from_id, to_id, relation, created_at) VALUES (?1, ?2, 'related', ?3)",
            )
            .bind(memory.id)
            .bind(target)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;
            if result.rows_affected() > 0 {
                added += 1;
            }
        }
        Ok(added)
    }

    /// Runs [`Db::link_references`] over every active memory: for data saved
    /// before links existed. Returns how many links were added.
    ///
    /// # Errors
    ///
    /// Returns an error if a memory cannot be read or linked.
    pub async fn relink_all(&self) -> CoreResult<usize> {
        let ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM memories WHERE superseded_by IS NULL")
                .fetch_all(&self.pool)
                .await?;
        let mut added = 0;
        for id in ids {
            added += self.link_references(&self.get_memory(id).await?).await?;
        }
        Ok(added)
    }

    /// Archives session summaries older than the newest [`KEEP_SESSIONS`] of
    /// the same project. Only the latest reaches the agent at session start,
    /// so the rest are history: kept, readable by id, out of search.
    ///
    /// # Errors
    ///
    /// Returns an error if the update fails.
    pub async fn archive_old_sessions(&self, newest: &Memory) -> CoreResult<u64> {
        if newest.kind != MemoryKind::SessionSummary {
            return Ok(0);
        }
        let ids: Vec<Uuid> = sqlx::query_scalar(
            r"
            SELECT id FROM memories
            WHERE kind = 'session_summary' AND pinned = 0 AND superseded_by IS NULL
              AND ((?1 IS NOT NULL AND project_id = ?1)
                   OR (?1 IS NULL AND project_id IS NULL AND project_name = ?2))
            ORDER BY updated_at DESC, created_at DESC, id DESC
            ",
        )
        .bind(newest.project_id)
        .bind(newest.project_name.clone().unwrap_or_default())
        .fetch_all(&self.pool)
        .await?;
        let mut archived = 0;
        for id in ids.into_iter().skip(KEEP_SESSIONS) {
            if id == newest.id {
                continue;
            }
            archived += sqlx::query("UPDATE memories SET superseded_by = ?1 WHERE id = ?2")
                .bind(newest.id)
                .bind(id)
                .execute(&self.pool)
                .await?
                .rows_affected();
        }
        Ok(archived)
    }

    /// Merges several memories into one. The survivor (the one with a
    /// `topic_key`, else the newest) takes the new title and content, the sum
    /// of their uses and their links; the others are archived pointing to it.
    /// Nothing is deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if fewer than two distinct memories are given, they
    /// belong to different projects, one is already archived, or the text is
    /// invalid.
    pub async fn consolidate_memories(&self, input: ConsolidateInput) -> CoreResult<Memory> {
        let mut seen = HashSet::new();
        let ids: Vec<Uuid> = input
            .ids
            .into_iter()
            .filter(|id| seen.insert(*id))
            .collect();
        if ids.len() < 2 || ids.len() > MAX_CONSOLIDATE {
            return Err(CoreError::Validation(format!(
                "consolidate between 2 and {MAX_CONSOLIDATE} distinct memories"
            )));
        }
        let title = text_with_limit("title", &input.title, MAX_MEMORY_TITLE_CHARS)?;
        let content = text_with_limit("content", &input.content, MAX_MEMORY_CONTENT_CHARS)?;

        let mut memories = Vec::with_capacity(ids.len());
        for id in &ids {
            let memory = self.get_memory(*id).await?;
            if let Some(by) = memory.superseded_by {
                return Err(CoreError::Validation(format!(
                    "memory {} was already consolidated into {}",
                    short_id(memory.id),
                    short_id(by)
                )));
            }
            memories.push(memory);
        }
        let first = &memories[0];
        if memories
            .iter()
            .any(|m| m.project_id != first.project_id || m.project_name != first.project_name)
        {
            return Err(CoreError::Validation(
                "only memories of the same project (or all global) can be consolidated".to_string(),
            ));
        }

        let target_index = memories
            .iter()
            .position(|m| m.topic_key.is_some())
            .unwrap_or_else(|| {
                memories
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, m)| m.updated_at)
                    .map_or(0, |(i, _)| i)
            });
        let target = memories[target_index].clone();
        let others: Vec<&Memory> = memories.iter().filter(|m| m.id != target.id).collect();
        let project_name = target.project_name.clone().unwrap_or_default();
        let topic_key = input
            .topic_key
            .as_deref()
            .map(|raw| normalize_topic_key(raw, &project_name))
            .filter(|k| !k.is_empty())
            .or_else(|| target.topic_key.clone())
            .or_else(|| others.iter().find_map(|m| m.topic_key.clone()))
            .unwrap_or_default();
        let kind = input.kind.unwrap_or(target.kind);
        let pinned = memories.iter().any(|m| m.pinned);
        let use_count: i64 = memories.iter().map(|m| m.use_count).sum();
        let last_used_at = memories.iter().filter_map(|m| m.last_used_at).max();
        let now = Utc::now();

        let mut tx = self.pool.begin().await?;
        for other in &others {
            // Free the topic key first: the survivor may take it over.
            sqlx::query("UPDATE memories SET superseded_by = ?1, topic_key = '' WHERE id = ?2")
                .bind(target.id)
                .bind(other.id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            r"
            UPDATE memories
            SET title = ?1, content = ?2, kind = ?3, topic_key = ?4, pinned = ?5,
                use_count = ?6, last_used_at = ?7, updated_at = ?8
            WHERE id = ?9
            ",
        )
        .bind(&title)
        .bind(&content)
        .bind(kind)
        .bind(&topic_key)
        .bind(pinned)
        .bind(use_count)
        .bind(last_used_at)
        .bind(now)
        .bind(target.id)
        .execute(&mut *tx)
        .await?;
        let other_ids: Vec<Uuid> = others.iter().map(|m| m.id).collect();
        move_links(&mut tx, target.id, &other_ids, now).await?;
        tx.commit().await?;
        self.get_memory(target.id).await
    }
}

/// Hands the absorbed memories' links to the survivor, records `supersedes`
/// and drops links among the merged memories themselves.
async fn move_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    target: Uuid,
    others: &[Uuid],
    now: chrono::DateTime<Utc>,
) -> CoreResult<()> {
    for &other in others {
        // The survivor inherits the absorbed memory's links...
        sqlx::query(
            r"
                INSERT OR IGNORE INTO memory_links (from_id, to_id, relation, created_at)
                SELECT ?1, to_id, relation, created_at FROM memory_links
                WHERE from_id = ?2 AND to_id <> ?1
                ",
        )
        .bind(target)
        .bind(other)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r"
                INSERT OR IGNORE INTO memory_links (from_id, to_id, relation, created_at)
                SELECT from_id, ?1, relation, created_at FROM memory_links
                WHERE to_id = ?2 AND from_id <> ?1
                ",
        )
        .bind(target)
        .bind(other)
        .execute(&mut **tx)
        .await?;
        // ...and records where it came from.
        sqlx::query(
                "INSERT OR IGNORE INTO memory_links (from_id, to_id, relation, created_at) VALUES (?1, ?2, 'supersedes', ?3)",
            )
            .bind(target)
            .bind(other)
            .bind(now)
            .execute(&mut **tx)
            .await?;
    }
    // Links among the merged memories themselves point at archived rows
    // of the same knowledge: drop them, keep only `supersedes`.
    for &other in others {
        sqlx::query(
                "DELETE FROM memory_links WHERE from_id = ?1 AND to_id = ?2 AND relation <> 'supersedes'",
            )
            .bind(target)
            .bind(other)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM memory_links WHERE from_id = ?1 AND to_id = ?2")
            .bind(other)
            .bind(target)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{contains_token, id_mentions, short_id};
    use uuid::Uuid;

    #[test]
    fn short_id_is_the_random_tail() {
        let id = Uuid::parse_str("01a2141c-0d24-749a-a8e4-b475da924a10").expect("uuid");
        assert_eq!(short_id(id), "da924a10");
    }

    #[test]
    fn topic_tokens_need_word_boundaries() {
        assert!(contains_token(
            "ver opencode-plugin-api-v2.",
            "opencode-plugin-api-v2"
        ));
        assert!(!contains_token(
            "ver opencode-plugin-api-v2x",
            "opencode-plugin-api-v2"
        ));
        assert!(!contains_token("pre-memory-scope", "memory-scope"));
    }

    #[test]
    fn finds_full_and_short_ids_but_not_plain_words() {
        let found =
            id_mentions("Ver 01a2141c-0d24-749a-a8e4-b475da924a10 y da924a10; commit ce23f39");
        assert!(found.contains(&"01a2141c0d24749aa8e4b475da924a10".to_string()));
        assert!(found.contains(&"da924a10".to_string()));
        assert!(
            !found.contains(&"ce23f39".to_string()),
            "7-digit git hashes are not ids"
        );
        assert!(id_mentions("id 12345678").contains(&"12345678".to_string()));
    }
}
