use sqlx::SqlitePool;
use tonic::{Request, Response, Status};

use crate::error::AppError;
use crate::proto::{
    search_service_server::SearchService as SearchServiceTrait, SearchHit, SearchRequest,
    SearchResponse,
};
use crate::services::entity::load_entity;

const DEFAULT_LIMIT: i64 = 10;

/// Turn a raw user query into a prefix-matching FTS5 MATCH expression so a
/// partial term like "trans" finds "Transformers". Each whitespace-separated
/// term is stripped of FTS operator chars, wrapped in double-quotes (so it is a
/// literal string token), and suffixed with `*` for a prefix scan. Terms are
/// space-joined → implicit AND. Returns "" when nothing usable remains.
fn to_prefix_match(query: &str) -> String {
    query
        .split_whitespace()
        .filter_map(|term| {
            let cleaned: String = term.chars().filter(|c| c.is_alphanumeric()).collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(format!("\"{cleaned}\"*"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct SearchService {
    pool: SqlitePool,
}

impl SearchService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[tonic::async_trait]
impl SearchServiceTrait for SearchService {
    /// Lexical search over the `entity_fts` FTS5 index (name + body). Ranks by
    /// bm25 (lower is better) and returns a `snippet()` over whichever column
    /// matched. Score is `-bm25` so callers can sort descending = most relevant.
    async fn search(
        &self,
        req: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let r = req.into_inner();
        let query = r.query.trim();
        if query.is_empty() {
            return Ok(Response::new(SearchResponse { hits: vec![] }));
        }
        let limit = if r.limit == 0 {
            DEFAULT_LIMIT
        } else {
            r.limit as i64
        };

        let match_query = to_prefix_match(query);
        if match_query.is_empty() {
            return Ok(Response::new(SearchResponse { hits: vec![] }));
        }

        // Runtime (non-macro) query: sqlx's compile-time introspection chokes on
        // FTS5 virtual tables (bm25/snippet/MATCH), so this query is unchecked.
        // snippet(table, column_index, start, end, ellipsis, max_tokens):
        // column -1 lets FTS pick the best-matching column for the snippet.
        let rows = sqlx::query_as::<_, (String, String, f64)>(
            r#"SELECT entity_id,
                      snippet(entity_fts, -1, '[', ']', '…', 12),
                      bm25(entity_fts)
               FROM entity_fts
               WHERE entity_fts MATCH ?
               ORDER BY bm25(entity_fts)
               LIMIT ?"#,
        )
        .bind(&match_query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        let mut hits = Vec::with_capacity(rows.len());
        for (entity_id, snippet, bm25) in rows {
            // The entity may have been deleted out from under a stale index row;
            // skip rather than fail the whole query.
            let entity = match load_entity(&self.pool, &entity_id).await {
                Ok(e) => e,
                Err(AppError::NotFound(_)) => continue,
                Err(e) => return Err(e.into()),
            };
            hits.push(SearchHit {
                entity: Some(entity),
                snippet,
                score: -bm25,
            });
        }

        Ok(Response::new(SearchResponse { hits }))
    }
}

#[cfg(test)]
mod tests {
    use super::to_prefix_match;

    #[test]
    fn builds_prefix_match_with_implicit_and() {
        assert_eq!(to_prefix_match("trans"), "\"trans\"*");
        assert_eq!(to_prefix_match("attention model"), "\"attention\"* \"model\"*");
    }

    #[test]
    fn strips_operators_and_blank_query() {
        assert_eq!(to_prefix_match("  \"OR\" * "), "\"OR\"*");
        assert_eq!(to_prefix_match("   "), "");
        assert_eq!(to_prefix_match("()*-"), "");
    }
}
