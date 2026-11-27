use crate::db::Db;
use crate::db::memory::{
    BORDERLINE_TITLE_SIMILARITY, MEMORY_COLUMNS, MIN_CONTENT_OVERLAP, MIN_SHARED_CONTENT_WORDS,
    MIN_TITLE_SIMILARITY, memory_tokens, title_words,
};
use crate::error::CoreResult;
use crate::models::{Memory, MemoryScope};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const CONTENT_BATCH_SIZE: usize = 500;
const PER_MEMORY_MATCH_CAP: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicatePair {
    pub a: Memory,
    pub b: Memory,
}

#[derive(Clone, sqlx::FromRow)]
struct DupeHeaderRow {
    id: Uuid,
    project_id: Option<Uuid>,
    project_name: Option<String>,
    title: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ContentRow {
    id: Uuid,
    content: String,
}

#[derive(Clone, Copy)]
enum CandidateKind {
    Confirmed(usize),
    Borderline(usize),
}

struct CandidatePair {
    a_idx: usize,
    b_idx: usize,
    a_updated: DateTime<Utc>,
    a_created: DateTime<Utc>,
    b_updated: DateTime<Utc>,
    a_id: Uuid,
}

fn is_global(project_id: Option<Uuid>, project_name: Option<&str>) -> bool {
    project_id.is_none() && project_name.unwrap_or("").is_empty()
}

fn same_project(a: &DupeHeaderRow, b: &DupeHeaderRow) -> bool {
    match (a.project_id, b.project_id) {
        (Some(pa), Some(pb)) => pa == pb,
        (None, None) => {
            is_global(a.project_id, a.project_name.as_deref())
                && is_global(b.project_id, b.project_name.as_deref())
        }
        _ => false,
    }
}

fn pair_key(a_id: Uuid, b_id: Uuid) -> (Uuid, Uuid) {
    if a_id < b_id {
        (a_id, b_id)
    } else {
        (b_id, a_id)
    }
}

fn order_pair_indices(rows: &[DupeHeaderRow], i: usize, j: usize) -> (usize, usize) {
    let a = &rows[i];
    let b = &rows[j];
    if (a.updated_at, a.created_at, a.id) >= (b.updated_at, b.created_at, b.id) {
        (i, j)
    } else {
        (j, i)
    }
}

fn intersection_len_sorted(a: &[u32], b: &[u32]) -> usize {
    let (mut ia, mut ib, mut shared) = (0_usize, 0_usize, 0_usize);
    while ia < a.len() && ib < b.len() {
        match a[ia].cmp(&b[ib]) {
            std::cmp::Ordering::Less => ia += 1,
            std::cmp::Ordering::Greater => ib += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                ia += 1;
                ib += 1;
            }
        }
    }
    shared
}

fn jaccard_sorted(a: &[u32], b: &[u32]) -> f64 {
    let shared = intersection_len_sorted(a, b);
    let union = a.len() + b.len() - shared;
    if union == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = shared as f64 / union as f64;
    ratio
}

fn combined_overlap_matches(a: &HashSet<String>, b: &HashSet<String>) -> bool {
    let shared = a.intersection(b).count();
    if shared < MIN_SHARED_CONTENT_WORDS {
        return false;
    }
    let union = a.len() + b.len() - shared;
    if union == 0 {
        return false;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = shared as f64 / union as f64;
    ratio >= MIN_CONTENT_OVERLAP
}

fn build_word_index(rows: &[DupeHeaderRow]) -> (Vec<Vec<u32>>, Vec<Vec<usize>>) {
    let mut vocab: HashMap<String, u32> = HashMap::new();
    let mut word_ids = Vec::with_capacity(rows.len());
    let mut inverted: Vec<Vec<usize>> = Vec::new();

    for (idx, row) in rows.iter().enumerate() {
        let words = title_words(&row.title);
        let mut ids = Vec::with_capacity(words.len());
        for word in words {
            let next_id = u32::try_from(vocab.len()).unwrap_or(u32::MAX);
            let wid = *vocab.entry(word).or_insert_with(|| {
                inverted.push(Vec::new());
                next_id
            });
            ids.push(wid);
        }
        ids.sort_unstable();
        ids.dedup();
        for &wid in &ids {
            inverted[wid as usize].push(idx);
        }
        word_ids.push(ids);
    }

    (word_ids, inverted)
}

fn collect_candidates(
    rows: &[DupeHeaderRow],
    word_ids: &[Vec<u32>],
    inverted: &[Vec<usize>],
) -> (Vec<Vec<CandidateKind>>, HashSet<usize>) {
    let mut per_row: Vec<Vec<CandidateKind>> = vec![Vec::new(); rows.len()];
    let mut borderline_indices = HashSet::new();
    let mut candidate_marks = vec![0_usize; rows.len()];
    let mut mark = 0_usize;
    let mut ordered_terms: Vec<u32> = Vec::new();
    let mut candidates: Vec<usize> = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        let terms = &word_ids[i];
        if terms.is_empty() {
            continue;
        }
        mark += 1;
        ordered_terms.clear();
        ordered_terms.extend_from_slice(terms);
        ordered_terms.sort_unstable_by_key(|&wid| inverted[wid as usize].len());

        let min_shared = (terms.len() * 3).div_ceil(10).max(1);
        let prefix_len = terms.len().saturating_sub(min_shared) + 1;

        candidates.clear();
        for &wid in ordered_terms.iter().take(prefix_len) {
            for &j in &inverted[wid as usize] {
                if j != i && candidate_marks[j] != mark {
                    candidate_marks[j] = mark;
                    candidates.push(j);
                }
            }
        }
        candidates.sort_unstable();

        let mut confirmed = 0_usize;
        let mut row_borderline = Vec::new();
        for &j in &candidates {
            if !same_project(row, &rows[j]) {
                continue;
            }
            let sim = jaccard_sorted(terms, &word_ids[j]);
            if sim >= MIN_TITLE_SIMILARITY {
                per_row[i].push(CandidateKind::Confirmed(j));
                confirmed += 1;
                if confirmed >= PER_MEMORY_MATCH_CAP {
                    break;
                }
            } else if sim >= BORDERLINE_TITLE_SIMILARITY {
                per_row[i].push(CandidateKind::Borderline(j));
                row_borderline.push(j);
            }
        }
        if !row_borderline.is_empty() {
            borderline_indices.insert(i);
            borderline_indices.extend(row_borderline);
        }
    }

    (per_row, borderline_indices)
}

impl Db {
    async fn fetch_dupe_headers(&self, scope: MemoryScope) -> CoreResult<Vec<DupeHeaderRow>> {
        let (mode, project_id) = match scope {
            MemoryScope::All => (0, None),
            MemoryScope::GlobalOnly => (1, None),
            MemoryScope::Project(id) => (2, Some(id)),
        };
        let rows = sqlx::query_as::<_, DupeHeaderRow>(
            r"
            SELECT id,
                   project_id,
                   NULLIF(project_name, '') AS project_name,
                   title,
                   created_at,
                   updated_at
            FROM memories
            WHERE superseded_by IS NULL
              AND kind <> 'session_summary'
              AND (?1 = 0
                   OR (project_id IS NULL AND project_name = '')
                   OR (?1 = 2 AND project_id = ?2))
            ORDER BY updated_at DESC, created_at DESC, id DESC
            ",
        )
        .bind(mode)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn fetch_contents_map(&self, ids: &[Uuid]) -> CoreResult<HashMap<Uuid, String>> {
        let mut map = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(CONTENT_BATCH_SIZE) {
            let mut qb = QueryBuilder::new("SELECT id, content FROM memories WHERE id IN (");
            let mut separated = qb.separated(", ");
            for id in chunk {
                separated.push_bind(*id);
            }
            separated.push_unseparated(")");
            let rows = qb
                .build_query_as::<ContentRow>()
                .fetch_all(&self.pool)
                .await?;
            for row in rows {
                map.insert(row.id, row.content);
            }
        }
        Ok(map)
    }

    async fn fetch_memories_map(&self, ids: &[Uuid]) -> CoreResult<HashMap<Uuid, Memory>> {
        let mut map = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(CONTENT_BATCH_SIZE) {
            let mut qb = QueryBuilder::new(format!(
                "SELECT {MEMORY_COLUMNS} FROM memories WHERE id IN ("
            ));
            let mut separated = qb.separated(", ");
            for id in chunk {
                separated.push_bind(*id);
            }
            separated.push_unseparated(")");
            let rows = qb.build_query_as::<Memory>().fetch_all(&self.pool).await?;
            for row in rows {
                map.insert(row.id, row);
            }
        }
        Ok(map)
    }

    /// # Errors
    ///
    /// Returns an error if the memories cannot be loaded.
    pub async fn duplicate_pairs(
        &self,
        scope: MemoryScope,
        limit: usize,
    ) -> CoreResult<Vec<DuplicatePair>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let rows = self.fetch_dupe_headers(scope).await?;
        if rows.len() < 2 {
            return Ok(Vec::new());
        }

        let (word_ids, inverted) = build_word_index(&rows);
        let (per_row_candidates, borderline_indices) =
            collect_candidates(&rows, &word_ids, &inverted);

        let content_tokens: HashMap<usize, HashSet<String>> = if borderline_indices.is_empty() {
            HashMap::new()
        } else {
            let ids: Vec<Uuid> = borderline_indices.iter().map(|&idx| rows[idx].id).collect();
            let contents = self.fetch_contents_map(&ids).await?;
            borderline_indices
                .into_iter()
                .filter_map(|idx| {
                    let content = contents.get(&rows[idx].id)?;
                    Some((idx, memory_tokens(&rows[idx].title, content)))
                })
                .collect()
        };

        let mut candidate_pairs = Vec::new();
        let mut seen = HashSet::new();

        for (i, candidates) in per_row_candidates.iter().enumerate() {
            let mut matched = 0_usize;
            for cand in candidates {
                let (j, is_match) = match *cand {
                    CandidateKind::Confirmed(j) => (j, true),
                    CandidateKind::Borderline(j) => {
                        let ok = match (content_tokens.get(&i), content_tokens.get(&j)) {
                            (Some(ta), Some(tb)) => combined_overlap_matches(ta, tb),
                            _ => false,
                        };
                        (j, ok)
                    }
                };
                if !is_match {
                    continue;
                }
                matched += 1;
                let key = pair_key(rows[i].id, rows[j].id);
                if seen.insert(key) {
                    let (a_idx, b_idx) = order_pair_indices(&rows, i, j);
                    candidate_pairs.push(CandidatePair {
                        a_idx,
                        b_idx,
                        a_updated: rows[a_idx].updated_at,
                        a_created: rows[a_idx].created_at,
                        b_updated: rows[b_idx].updated_at,
                        a_id: rows[a_idx].id,
                    });
                }
                if matched >= PER_MEMORY_MATCH_CAP {
                    break;
                }
            }
        }

        candidate_pairs.sort_by(|p1, p2| {
            p2.a_updated
                .cmp(&p1.a_updated)
                .then_with(|| p2.a_created.cmp(&p1.a_created))
                .then_with(|| p2.b_updated.cmp(&p1.b_updated))
                .then_with(|| p1.a_id.cmp(&p2.a_id))
        });
        candidate_pairs.truncate(limit);

        if candidate_pairs.is_empty() {
            return Ok(Vec::new());
        }

        let mut needed_ids = Vec::with_capacity(candidate_pairs.len() * 2);
        let mut id_set = HashSet::with_capacity(candidate_pairs.len() * 2);
        for pair in &candidate_pairs {
            for idx in [pair.a_idx, pair.b_idx] {
                let id = rows[idx].id;
                if id_set.insert(id) {
                    needed_ids.push(id);
                }
            }
        }

        let memories = self.fetch_memories_map(&needed_ids).await?;
        let mut pairs = Vec::with_capacity(candidate_pairs.len());
        for pair in candidate_pairs {
            if let (Some(a), Some(b)) = (
                memories.get(&rows[pair.a_idx].id),
                memories.get(&rows[pair.b_idx].id),
            ) {
                pairs.push(DuplicatePair {
                    a: a.clone(),
                    b: b.clone(),
                });
            }
        }
        Ok(pairs)
    }
}
