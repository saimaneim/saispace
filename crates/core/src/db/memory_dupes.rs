use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::models::{Memory, MemoryKind, MemoryScope};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicatePair {
    pub a: Memory,
    pub b: Memory,
}

fn is_global(m: &Memory) -> bool {
    m.project_id.is_none() && m.project_name.as_deref().unwrap_or("").is_empty()
}

fn same_project(a: &Memory, b: &Memory) -> bool {
    match (a.project_id, b.project_id) {
        (Some(pa), Some(pb)) => pa == pb,
        (None, None) => is_global(a) && is_global(b),
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

fn make_pair(a: Memory, b: Memory) -> DuplicatePair {
    if (a.updated_at, a.created_at, a.id) >= (b.updated_at, b.created_at, b.id) {
        DuplicatePair { a, b }
    } else {
        DuplicatePair { a: b, b: a }
    }
}

impl Db {
    pub async fn duplicate_pairs(
        &self,
        scope: MemoryScope,
        limit: usize,
    ) -> CoreResult<Vec<DuplicatePair>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let memories = self.recent_memories(scope, 200).await?;
        let mut pairs = Vec::new();
        let mut seen = HashSet::new();

        for m in memories {
            if m.kind == MemoryKind::SessionSummary {
                continue;
            }

            let similar = match self.similar_memories(&m, 3).await {
                Ok(hits) => hits,
                Err(CoreError::Validation(_)) => continue,
                Err(err) => return Err(err),
            };

            for hit in similar {
                let key = pair_key(m.id, hit.id);
                if seen.contains(&key) {
                    continue;
                }

                let other = self.get_memory(hit.id).await?;
                if other.kind == MemoryKind::SessionSummary {
                    continue;
                }

                if !same_project(&m, &other) {
                    continue;
                }

                seen.insert(key);
                pairs.push(make_pair(m.clone(), other));
            }
        }

        pairs.sort_by(|p1, p2| {
            p2.a.updated_at
                .cmp(&p1.a.updated_at)
                .then_with(|| p2.a.created_at.cmp(&p1.a.created_at))
                .then_with(|| p2.b.updated_at.cmp(&p1.b.updated_at))
                .then_with(|| p1.a.id.cmp(&p2.a.id))
        });

        pairs.truncate(limit);
        Ok(pairs)
    }
}
