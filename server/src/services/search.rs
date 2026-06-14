use std::collections::HashMap;

use sqlx::SqlitePool;
use tonic::{Request, Response, Status};

use crate::embed::{vector_blob, EmbedHandle};
use crate::error::AppError;
use crate::proto::{
    search_service_server::SearchService as SearchServiceTrait, RetrieveRequest, SearchHit,
    SearchRequest, SearchResponse,
};
use crate::services::entity::load_entity;

const DEFAULT_LIMIT: i64 = 10;

/// Reciprocal-rank-fusion constant. The standard k=60 from the RRF paper: damps
/// the contribution of low-ranked items so the head of each list dominates.
const RRF_K: f64 = 60.0;

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
    embed: EmbedHandle,
}

impl SearchService {
    pub fn new(pool: SqlitePool, embed: EmbedHandle) -> Self {
        Self { pool, embed }
    }

    /// FTS5 lexical hits as ordered (entity_id, snippet) pairs, best first.
    /// Shared by `Search` and the hybrid branch of `Retrieve`.
    async fn fts_hits(&self, query: &str, limit: i64) -> Result<Vec<(String, String)>, AppError> {
        let match_query = to_prefix_match(query);
        if match_query.is_empty() {
            return Ok(vec![]);
        }
        // Runtime (non-macro) query: sqlx's compile-time introspection chokes on
        // FTS5 virtual tables (bm25/snippet/MATCH), so this query is unchecked.
        // snippet column -1 lets FTS pick the best-matching column.
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"SELECT entity_id,
                      snippet(entity_fts, -1, '[', ']', '…', 12)
               FROM entity_fts
               WHERE entity_fts MATCH ?
               ORDER BY bm25(entity_fts)
               LIMIT ?"#,
        )
        .bind(&match_query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Vector KNN over chunk embeddings, collapsed to best-chunk-per-entity and
    /// returned as an ordered list of (entity_id, distance), nearest first.
    /// `query_vec` is the already-embedded search string. Over-fetches chunks
    /// (k * 4) before collapsing so several chunks of one entity don't crowd out
    /// other entities from the top-k.
    async fn vector_hits(
        &self,
        query_vec: &[f32],
        k: i64,
    ) -> Result<Vec<(String, f64)>, AppError> {
        let blob = vector_blob(query_vec);
        let rows = sqlx::query_as::<_, (String, f64)>(
            r#"SELECT c.entity_id, v.distance
               FROM entity_vec v
               JOIN chunks c ON c.id = v.id
               WHERE v.embedding MATCH ? AND k = ?
               ORDER BY v.distance"#,
        )
        .bind(&blob[..])
        .bind(k * 4)
        .fetch_all(&self.pool)
        .await?;

        // Collapse to the best (smallest-distance) chunk per entity, preserving
        // ascending-distance order.
        let mut best: HashMap<String, f64> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for (entity_id, distance) in rows {
            if !best.contains_key(&entity_id) {
                order.push(entity_id.clone());
            }
            best.entry(entity_id)
                .and_modify(|d| {
                    if distance < *d {
                        *d = distance;
                    }
                })
                .or_insert(distance);
        }
        Ok(order.into_iter().map(|id| (id.clone(), best[&id])).collect())
    }

    /// Hydrate ordered (entity_id, score, snippet) tuples into SearchHits,
    /// skipping entities deleted out from under a stale index.
    async fn hydrate(
        &self,
        ranked: Vec<(String, f64, String)>,
    ) -> Result<Vec<SearchHit>, Status> {
        let mut hits = Vec::with_capacity(ranked.len());
        for (entity_id, score, snippet) in ranked {
            let entity = match load_entity(&self.pool, &entity_id).await {
                Ok(e) => e,
                Err(AppError::NotFound(_)) => continue,
                Err(e) => return Err(e.into()),
            };
            hits.push(SearchHit {
                entity: Some(entity),
                snippet,
                score,
            });
        }
        Ok(hits)
    }
}

#[tonic::async_trait]
impl SearchServiceTrait for SearchService {
    /// Lexical search over the `entity_fts` FTS5 index (name + body). Ranks by
    /// bm25 (lower is better) and returns a `snippet()` over whichever column
    /// matched. Score is `-rank` so callers can sort descending = most relevant.
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

        let fts = self.fts_hits(query, limit).await.map_err(Status::from)?;
        // Score by position so callers get a stable descending ordering; the raw
        // bm25 value isn't surfaced (it never was — `Search` returns -bm25 order).
        let ranked: Vec<(String, f64, String)> = fts
            .into_iter()
            .enumerate()
            .map(|(i, (id, snippet))| (id, -(i as f64), snippet))
            .collect();
        let hits = self.hydrate(ranked).await?;
        Ok(Response::new(SearchResponse { hits }))
    }

    /// Semantic retrieval. Embeds the query and runs vector KNN over chunk
    /// embeddings; with `hybrid`, fuses that ranking with FTS5 via reciprocal-
    /// rank fusion (score = Σ 1/(RRF_K + rank) across the lists an entity appears
    /// in). Degrades to pure lexical `Search` when embeddings are disabled.
    async fn retrieve(
        &self,
        req: Request<RetrieveRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let r = req.into_inner();
        let query = r.query.trim().to_string();
        if query.is_empty() {
            return Ok(Response::new(SearchResponse { hits: vec![] }));
        }
        let k = if r.k == 0 { DEFAULT_LIMIT } else { r.k as i64 };

        // Embed the query. None => embeddings disabled => fall back to lexical so
        // the RPC still returns useful results instead of erroring.
        let query_vec = match self.embed.embed_query(&query).await.map_err(Status::from)? {
            Some(v) => v,
            None => {
                let fts = self.fts_hits(&query, k).await.map_err(Status::from)?;
                let ranked = fts
                    .into_iter()
                    .enumerate()
                    .map(|(i, (id, snippet))| (id, -(i as f64), snippet))
                    .collect();
                let hits = self.hydrate(ranked).await?;
                return Ok(Response::new(SearchResponse { hits }));
            }
        };

        let vector = self.vector_hits(&query_vec, k).await.map_err(Status::from)?;

        let ranked: Vec<(String, f64, String)> = if r.hybrid {
            let fts = self.fts_hits(&query, k).await.map_err(Status::from)?;
            rrf_fuse(&vector, &fts, k as usize)
        } else {
            // Pure vector: score = -distance (nearer = higher), no snippet.
            vector
                .into_iter()
                .take(k as usize)
                .map(|(id, dist)| (id, -dist, String::new()))
                .collect()
        };

        let hits = self.hydrate(ranked).await?;
        Ok(Response::new(SearchResponse { hits }))
    }
}

/// Reciprocal-rank fusion of the vector and FTS rankings. Each list contributes
/// `1/(RRF_K + rank)` (0-based rank) per entity; an entity present in both is
/// rewarded by the sum. Snippet comes from FTS when available, else empty.
/// Returns the top-`k` fused tuples, highest score first.
fn rrf_fuse(
    vector: &[(String, f64)],
    fts: &[(String, String)],
    k: usize,
) -> Vec<(String, f64, String)> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut snippets: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    let bump = |id: &str, rank: usize, scores: &mut HashMap<String, f64>, order: &mut Vec<String>| {
        if !scores.contains_key(id) {
            order.push(id.to_string());
        }
        *scores.entry(id.to_string()).or_insert(0.0) += 1.0 / (RRF_K + rank as f64);
    };

    for (rank, (id, _dist)) in vector.iter().enumerate() {
        bump(id, rank, &mut scores, &mut order);
    }
    for (rank, (id, snippet)) in fts.iter().enumerate() {
        bump(id, rank, &mut scores, &mut order);
        snippets.entry(id.clone()).or_insert_with(|| snippet.clone());
    }

    let mut fused: Vec<(String, f64, String)> = order
        .into_iter()
        .map(|id| {
            let score = scores[&id];
            let snippet = snippets.get(&id).cloned().unwrap_or_default();
            (id, score, snippet)
        })
        .collect();
    // Descending by fused score; ties keep first-seen order (stable sort).
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused.truncate(k);
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn rrf_rewards_agreement_across_lists() {
        let vector = vec![("a".to_string(), 0.1), ("b".to_string(), 0.2)];
        let fts = vec![("b".to_string(), "snip".to_string()), ("c".to_string(), "x".to_string())];
        let fused = rrf_fuse(&vector, &fts, 10);
        // `b` appears in both lists, so it should outrank a/c which appear once.
        assert_eq!(fused[0].0, "b");
        // Its snippet is carried over from FTS.
        assert_eq!(fused[0].2, "snip");
    }
}
