use crate::db::Db;
use crate::error::{CoreError, CoreResult};
use crate::models::{MemoryScope, SearchHit};
use uuid::Uuid;

const MAX_TERMS: usize = 16;
pub const DEFAULT_SEARCH_LIMIT: i64 = 20;
pub const MAX_SEARCH_LIMIT: i64 = 100;

/// Words that carry no meaning on their own; dropped unless nothing else is left.
pub(crate) const STOPWORDS: &[&str] = &[
    "a", "al", "como", "con", "de", "del", "el", "en", "es", "la", "las", "lo", "los", "o", "para",
    "pero", "por", "que", "se", "sin", "su", "sus", "un", "una", "unos", "unas", "y", "and", "for",
    "in", "is", "of", "on", "or", "the", "to", "with",
];

fn build_match_query(query: &str, any: bool) -> CoreResult<String> {
    let terms: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let meaningful: Vec<&str> = terms
        .iter()
        .copied()
        .filter(|t| !STOPWORDS.contains(&t.to_lowercase().as_str()))
        .collect();
    let terms = if meaningful.is_empty() {
        terms
    } else {
        meaningful
    };
    let terms = &terms[..terms.len().min(MAX_TERMS)];
    let Some((last, head)) = terms.split_last() else {
        return Err(CoreError::Validation(
            "search query must contain at least one letter or digit".to_string(),
        ));
    };
    let mut parts: Vec<String> = head.iter().map(|t| format!("\"{t}\"")).collect();
    parts.push(format!("\"{last}\"*"));
    Ok(parts.join(if any { " OR " } else { " " }))
}

impl Db {
    /// Searches everything (`None`) or one project plus global rows.
    ///
    /// # Errors
    ///
    /// Returns an error if the search query fails.
    pub async fn search(
        &self,
        query: &str,
        project_id: Option<Uuid>,
        limit: Option<i64>,
    ) -> CoreResult<Vec<SearchHit>> {
        let scope = project_id.map_or(MemoryScope::All, MemoryScope::Project);
        self.search_scoped(query, scope, limit).await
    }

    /// Matches every term first; when nothing matches, retries with any term
    /// ranked by relevance, so one missing word does not hide the answer.
    ///
    /// # Errors
    ///
    /// Returns an error if the search query fails.
    pub async fn search_scoped(
        &self,
        query: &str,
        scope: MemoryScope,
        limit: Option<i64>,
    ) -> CoreResult<Vec<SearchHit>> {
        let limit = limit
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        let hits = self
            .run_search(&build_match_query(query, false)?, scope, limit)
            .await?;
        if !hits.is_empty() {
            return Ok(hits);
        }
        self.run_search(&build_match_query(query, true)?, scope, limit)
            .await
    }

    /// Rows whose title shares any word with `title`.
    pub(crate) async fn search_titles(
        &self,
        title: &str,
        scope: MemoryScope,
        limit: i64,
    ) -> CoreResult<Vec<SearchHit>> {
        self.run_search(
            &format!("title : ({})", build_match_query(title, true)?),
            scope,
            limit.clamp(1, MAX_SEARCH_LIMIT),
        )
        .await
    }

    async fn run_search(
        &self,
        match_query: &str,
        scope: MemoryScope,
        limit: i64,
    ) -> CoreResult<Vec<SearchHit>> {
        let (mode, project_id) = match scope {
            MemoryScope::All => (0, None),
            MemoryScope::GlobalOnly => (1, None),
            MemoryScope::Project(id) => (2, Some(id)),
        };
        // A memory without project_id but with a project_name belongs to a
        // project that is not registered yet: it is not global.
        let hits = sqlx::query_as::<_, SearchHit>(
            r"
            SELECT kind,
                   ref_id AS id,
                   project_id,
                   title,
                   snippet(search_index, -1, '[', ']', '…', 12) AS snippet,
                   bm25(search_index, 0.0, 0.0, 0.0, 4.0, 1.0) AS rank
            FROM search_index
            WHERE search_index MATCH ?1
              AND (
                ?2 = 0
                OR (?2 = 2 AND project_id = ?3)
                OR (project_id IS NULL AND NOT (kind = 'memory' AND EXISTS (
                      SELECT 1 FROM memories m
                      WHERE m.id = search_index.ref_id AND m.project_name <> '')))
              )
              -- Consolidated memories live on in the memory that absorbed them.
              AND NOT (kind = 'memory' AND EXISTS (
                    SELECT 1 FROM memories m
                    WHERE m.id = search_index.ref_id AND m.superseded_by IS NOT NULL))
            ORDER BY rank
            LIMIT ?4
            ",
        )
        .bind(match_query)
        .bind(mode)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::build_match_query;

    #[test]
    fn quotes_terms_and_prefixes_the_last() {
        assert_eq!(
            build_match_query("tauri v2 perm", false).ok().as_deref(),
            Some("\"tauri\" \"v2\" \"perm\"*")
        );
        assert_eq!(
            build_match_query("x\" NEAR b*", false).ok().as_deref(),
            Some("\"x\" \"NEAR\" \"b\"*")
        );
        assert!(build_match_query("  -- \"\" ", false).is_err());
    }

    #[test]
    fn drops_stopwords_and_joins_with_or_when_asked() {
        assert_eq!(
            build_match_query("desplegar el CRM en la NAS", false)
                .ok()
                .as_deref(),
            Some("\"desplegar\" \"CRM\" \"NAS\"*")
        );
        assert_eq!(
            build_match_query("cifrado claves", true).ok().as_deref(),
            Some("\"cifrado\" OR \"claves\"*")
        );
        assert_eq!(
            build_match_query("de la", false).ok().as_deref(),
            Some("\"de\" \"la\"*")
        );
    }
}
