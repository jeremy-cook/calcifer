use std::collections::HashMap;

use sqlx::SqlitePool;
use tonic::{Request, Response, Status};

use crate::embed::{vector_blob, EmbedHandle};
use crate::error::AppError;
use crate::proto::{
    search_service_server::SearchService as SearchServiceTrait, MatchRange, SearchHit, SearchMode,
    SearchRequest, SearchResponse,
};
use crate::services::entity::load_entities_by_id;

const DEFAULT_LIMIT: i64 = 10;

/// Reciprocal-rank-fusion constant. The standard k=60 from the RRF paper: damps
/// the contribution of low-ranked items so the head of each list dominates.
const RRF_K: f64 = 60.0;

/// What FTS5's `snippet()` wraps each matched term in. Private-use code points,
/// so they don't collide with anything a note means (unlike `[`…`]`, which reads
/// as wikilink syntax). `Snippet::parse` strips them into `MatchRange`s.
const MATCH_OPEN: char = '\u{E000}';
const MATCH_CLOSE: char = '\u{E001}';

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

/// A plain-text snippet and where the query matched in it.
#[derive(Clone, Debug, Default, PartialEq)]
struct Snippet {
    text: String,
    matches: Vec<MatchRange>,
}

impl Snippet {
    /// Strip `MATCH_OPEN`/`MATCH_CLOSE` from an FTS5 snippet, recording each
    /// marked span as a half-open range of UTF-16 code units into the stripped
    /// text. A marker out of place (which only a note containing one could cause)
    /// is dropped without opening or closing a range.
    fn parse(marked: &str) -> Self {
        let mut text = String::with_capacity(marked.len());
        let mut matches = Vec::new();
        let mut pos: u32 = 0;
        let mut open: Option<u32> = None;
        for c in marked.chars() {
            match c {
                MATCH_OPEN => {
                    if open.is_none() {
                        open = Some(pos);
                    }
                }
                MATCH_CLOSE => {
                    if let Some(start) = open.take() {
                        matches.push(MatchRange { start, end: pos });
                    }
                }
                _ => {
                    text.push(c);
                    pos += c.len_utf16() as u32;
                }
            }
        }
        Self { text, matches }
    }
}

/// An entity id, its score, and its snippet, in rank order before hydration.
type Ranked = Vec<(String, f64, Snippet)>;

pub struct SearchService {
    pool: SqlitePool,
    embed: EmbedHandle,
}

impl SearchService {
    pub fn new(pool: SqlitePool, embed: EmbedHandle) -> Self {
        Self { pool, embed }
    }

    /// FTS5 lexical hits as ordered (entity_id, snippet) pairs, best first.
    /// Shared by the lexical and hybrid modes.
    async fn fts_hits(&self, query: &str, limit: i64) -> Result<Vec<(String, Snippet)>, AppError> {
        let match_query = to_prefix_match(query);
        if match_query.is_empty() {
            return Ok(vec![]);
        }
        // Runtime (non-macro) query: sqlx's compile-time introspection chokes on
        // FTS5 virtual tables (bm25/snippet/MATCH), so this query is unchecked.
        // snippet column -1 lets FTS pick the best-matching column.
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"SELECT entity_id,
                      snippet(entity_fts, -1, ?, ?, '…', 12)
               FROM entity_fts
               WHERE entity_fts MATCH ?
               ORDER BY bm25(entity_fts)
               LIMIT ?"#,
        )
        .bind(MATCH_OPEN.to_string())
        .bind(MATCH_CLOSE.to_string())
        .bind(&match_query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, marked)| (id, Snippet::parse(&marked)))
            .collect())
    }

    /// Lexical ranking: FTS5 order, scored by position so callers get a stable
    /// descending ordering (the raw bm25 value isn't surfaced).
    async fn lexical(&self, query: &str, limit: i64) -> Result<Ranked, AppError> {
        let fts = self.fts_hits(query, limit).await?;
        Ok(fts
            .into_iter()
            .enumerate()
            .map(|(i, (id, snippet))| (id, -(i as f64), snippet))
            .collect())
    }

    /// Vector KNN over chunk embeddings, collapsed to best-chunk-per-entity and
    /// returned as an ordered list of (entity_id, distance), nearest first.
    /// `query_vec` is the already-embedded search string. Over-fetches chunks
    /// (k * 4) before collapsing so several chunks of one entity don't crowd out
    /// other entities from the top-k.
    async fn vector_hits(&self, query_vec: &[f32], k: i64) -> Result<Vec<(String, f64)>, AppError> {
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
        Ok(order
            .into_iter()
            .map(|id| (id.clone(), best[&id]))
            .collect())
    }

    /// Rank for `mode`. Semantic embeds the query and runs vector KNN; hybrid
    /// fuses that ranking with FTS5 via reciprocal-rank fusion (score =
    /// Σ 1/(RRF_K + rank) across the lists an entity appears in). Both degrade to
    /// lexical when embeddings are disabled. Unspecified means hybrid.
    async fn rank(&self, query: &str, limit: i64, mode: SearchMode) -> Result<Ranked, AppError> {
        let hybrid = match mode {
            SearchMode::Lexical => return self.lexical(query, limit).await,
            SearchMode::Semantic => false,
            SearchMode::Hybrid | SearchMode::Unspecified => true,
        };

        // None => embeddings disabled => fall back to lexical so the RPC still
        // returns useful results instead of erroring.
        let Some(query_vec) = self.embed.embed_query(query).await? else {
            return self.lexical(query, limit).await;
        };

        let vector = self.vector_hits(&query_vec, limit).await?;
        if hybrid {
            let fts = self.fts_hits(query, limit).await?;
            return Ok(rrf_fuse(&vector, &fts, limit as usize));
        }
        // Pure vector: score = -distance (nearer = higher), no snippet.
        Ok(vector
            .into_iter()
            .take(limit as usize)
            .map(|(id, dist)| (id, -dist, Snippet::default()))
            .collect())
    }

    /// Hydrate ranked tuples into SearchHits in one batch load, keeping rank
    /// order and skipping entities deleted out from under a stale index.
    async fn hydrate(&self, ranked: Ranked) -> Result<Vec<SearchHit>, AppError> {
        let ids: Vec<String> = ranked.iter().map(|(id, _, _)| id.clone()).collect();
        let mut entities: HashMap<String, _> = load_entities_by_id(&self.pool, &ids)
            .await?
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect();
        Ok(ranked
            .into_iter()
            .filter_map(|(id, score, snippet)| {
                let entity = entities.remove(&id)?;
                Some(SearchHit {
                    entity: Some(entity),
                    snippet: snippet.text,
                    score,
                    matches: snippet.matches,
                })
            })
            .collect())
    }
}

#[tonic::async_trait]
impl SearchServiceTrait for SearchService {
    /// Search in the request's mode (see `rank`). Lexical ranks by bm25 over the
    /// `entity_fts` FTS5 index (name + body) and returns a `snippet()` over
    /// whichever column matched. Scores sort descending = most relevant.
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
        // An unknown enum value (a newer client) is treated as unspecified.
        let mode = SearchMode::try_from(r.mode).unwrap_or(SearchMode::Unspecified);

        let ranked = self.rank(query, limit, mode).await?;
        let hits = self.hydrate(ranked).await?;
        Ok(Response::new(SearchResponse { hits }))
    }
}

/// Reciprocal-rank fusion of the vector and FTS rankings. Each list contributes
/// `1/(RRF_K + rank)` (0-based rank) per entity; an entity present in both is
/// rewarded by the sum. Snippet comes from FTS when available, else empty (so a
/// vector-only hit has no matches).
/// Returns the top-`k` fused tuples, highest score first.
fn rrf_fuse(vector: &[(String, f64)], fts: &[(String, Snippet)], k: usize) -> Ranked {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut snippets: HashMap<String, Snippet> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    let bump =
        |id: &str, rank: usize, scores: &mut HashMap<String, f64>, order: &mut Vec<String>| {
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
        snippets
            .entry(id.clone())
            .or_insert_with(|| snippet.clone());
    }

    let mut fused: Ranked = order
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
    use std::sync::Arc;

    use super::*;
    use crate::embed::provider::{Embedder, EMBED_DIM};
    use crate::proto::entity_service_server::EntityService as _;
    use crate::proto::Entity;
    use crate::services::entity::load_entity;
    use crate::test_support::{entity_service, memory_pool, note};

    /// Embeds every text as the unit vector on axis 0, so a chunk stored on axis
    /// 0 is the nearest neighbour of any query.
    struct AxisEmbedder;

    impl Embedder for AxisEmbedder {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
            Ok(texts.iter().map(|_| axis(0)).collect())
        }
    }

    fn axis(i: usize) -> Vec<f32> {
        let mut v = vec![0.0; EMBED_DIM];
        v[i] = 1.0;
        v
    }

    async fn create(pool: &SqlitePool, name: &str) -> Entity {
        entity_service(pool.clone())
            .create_entity(Request::new(note(name)))
            .await
            .expect("create")
            .into_inner()
            .entity
            .expect("entity")
    }

    /// Store one chunk for `entity_id` with `embedding`, as the embed worker would.
    async fn store_chunk(pool: &SqlitePool, entity_id: &str, embedding: &[f32]) {
        let chunk_id: i64 = sqlx::query_scalar(
            "INSERT INTO chunks (entity_id, chunk_index, text) VALUES (?, 0, '') RETURNING id",
        )
        .bind(entity_id)
        .fetch_one(pool)
        .await
        .expect("insert chunk");
        sqlx::query("INSERT INTO entity_vec (id, embedding) VALUES (?, ?)")
            .bind(chunk_id)
            .bind(vector_blob(embedding))
            .execute(pool)
            .await
            .expect("insert vector");
    }

    async fn search(svc: &SearchService, query: &str, mode: SearchMode) -> Vec<SearchHit> {
        svc.search(Request::new(SearchRequest {
            query: query.to_string(),
            limit: 0,
            mode: mode as i32,
        }))
        .await
        .expect("search")
        .into_inner()
        .hits
    }

    fn names(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter()
            .map(|h| h.entity.as_ref().unwrap().name.as_str())
            .collect()
    }

    /// The text a range covers, counting UTF-16 code units as clients do.
    fn covered(snippet: &str, range: &MatchRange) -> String {
        let units: Vec<u16> = snippet.encode_utf16().collect();
        String::from_utf16(&units[range.start as usize..range.end as usize]).unwrap()
    }

    #[test]
    fn builds_prefix_match_with_implicit_and() {
        assert_eq!(to_prefix_match("trans"), "\"trans\"*");
        assert_eq!(
            to_prefix_match("attention model"),
            "\"attention\"* \"model\"*"
        );
    }

    #[test]
    fn strips_operators_and_blank_query() {
        assert_eq!(to_prefix_match("  \"OR\" * "), "\"OR\"*");
        assert_eq!(to_prefix_match("   "), "");
        assert_eq!(to_prefix_match("()*-"), "");
    }

    #[test]
    fn rrf_rewards_agreement_across_lists() {
        let snip = Snippet::parse("\u{E000}snip\u{E001}");
        let vector = vec![("a".to_string(), 0.1), ("b".to_string(), 0.2)];
        let fts = vec![
            ("b".to_string(), snip.clone()),
            ("c".to_string(), Snippet::parse("x")),
        ];
        let fused = rrf_fuse(&vector, &fts, 10);
        // `b` appears in both lists, so it should outrank a/c which appear once.
        assert_eq!(fused[0].0, "b");
        // Its snippet is carried over from FTS.
        assert_eq!(fused[0].2, snip);
    }

    #[test]
    fn snippet_parse_drops_markers_out_of_place() {
        let s = Snippet::parse("a\u{E001}b\u{E000}c\u{E000}d\u{E001}e\u{E000}f");
        assert_eq!(s.text, "abcdef");
        // Only the first open counts; the stray close before it and the unclosed
        // open at the end make no range.
        assert_eq!(s.matches, vec![MatchRange { start: 2, end: 4 }]);
    }

    #[tokio::test]
    async fn match_ranges_count_utf16_units_after_non_ascii_text() {
        let pool = memory_pool().await;
        create(&pool, "Café 😀 Transformers").await;
        let svc = SearchService::new(pool, EmbedHandle::disabled());

        let hits = search(&svc, "trans", SearchMode::Lexical).await;

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet, "Café 😀 Transformers");
        // "Café " is 5 units and the emoji is a surrogate pair: 5 + 2 + 1 = 8.
        assert_eq!(hits[0].matches, vec![MatchRange { start: 8, end: 20 }]);
        assert_eq!(
            covered(&hits[0].snippet, &hits[0].matches[0]),
            "Transformers"
        );
    }

    #[tokio::test]
    async fn snippets_carry_no_bracket_markers() {
        let pool = memory_pool().await;
        create(&pool, "Attention models and attention heads").await;
        let svc = SearchService::new(pool, EmbedHandle::disabled());

        let hits = search(&svc, "attention", SearchMode::Lexical).await;

        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!(hit.snippet, "Attention models and attention heads");
        assert!(!hit.snippet.contains(['[', ']', MATCH_OPEN, MATCH_CLOSE]));
        let matched: Vec<String> = hit
            .matches
            .iter()
            .map(|m| covered(&hit.snippet, m))
            .collect();
        assert_eq!(matched, vec!["Attention", "attention"]);
    }

    #[tokio::test]
    async fn search_dispatches_on_mode() {
        let pool = memory_pool().await;
        // Near the query in vector space, but no keyword match.
        let sailboat = create(&pool, "Sailboat").await;
        store_chunk(&pool, &sailboat.id, &axis(0)).await;
        // A keyword match with no vector.
        create(&pool, "Vessel log").await;
        let svc = SearchService::new(pool, EmbedHandle::with_embedder(Arc::new(AxisEmbedder)));

        let lexical = search(&svc, "vessel", SearchMode::Lexical).await;
        assert_eq!(names(&lexical), vec!["Vessel log"]);
        assert_eq!(lexical[0].matches, vec![MatchRange { start: 0, end: 6 }]);

        let semantic = search(&svc, "vessel", SearchMode::Semantic).await;
        assert_eq!(names(&semantic), vec!["Sailboat"]);
        // A semantic hit with no lexical match has no snippet and no ranges.
        assert_eq!(semantic[0].snippet, "");
        assert!(semantic[0].matches.is_empty());

        let hybrid = search(&svc, "vessel", SearchMode::Hybrid).await;
        assert_eq!(names(&hybrid), vec!["Sailboat", "Vessel log"]);
        assert!(hybrid[0].matches.is_empty());
        assert_eq!(hybrid[1].matches, vec![MatchRange { start: 0, end: 6 }]);

        // Unspecified is hybrid.
        let unspecified = search(&svc, "vessel", SearchMode::Unspecified).await;
        assert_eq!(names(&unspecified), names(&hybrid));
    }

    #[tokio::test]
    async fn semantic_and_hybrid_fall_back_to_lexical_without_embeddings() {
        let pool = memory_pool().await;
        let sailboat = create(&pool, "Sailboat").await;
        store_chunk(&pool, &sailboat.id, &axis(0)).await;
        create(&pool, "Vessel log").await;
        let svc = SearchService::new(pool, EmbedHandle::disabled());

        let lexical = search(&svc, "vessel", SearchMode::Lexical).await;
        for mode in [
            SearchMode::Semantic,
            SearchMode::Hybrid,
            SearchMode::Unspecified,
        ] {
            assert_eq!(search(&svc, "vessel", mode).await, lexical, "{mode:?}");
        }
    }

    #[tokio::test]
    async fn hits_hydrate_as_load_entity_in_rank_order_and_skip_stale_rows() {
        let pool = memory_pool().await;
        create(&pool, "Transformers").await;
        create(&pool, "Transformers and transformers").await;
        create(&pool, "Transformer notes").await;
        // An index row whose entity is gone, as a stale index would leave.
        sqlx::query(
            "INSERT INTO entity_fts (entity_id, name, body) VALUES ('ghost', 'Transformers', '')",
        )
        .execute(&pool)
        .await
        .expect("insert stale fts row");
        let svc = SearchService::new(pool.clone(), EmbedHandle::disabled());

        let hits = search(&svc, "transformer", SearchMode::Lexical).await;
        let ranked = svc
            .lexical("transformer", DEFAULT_LIMIT)
            .await
            .expect("rank");

        let live: Vec<&String> = ranked
            .iter()
            .map(|(id, _, _)| id)
            .filter(|id| *id != "ghost")
            .collect();
        assert_eq!(ranked.len(), 4);
        assert_eq!(hits.len(), 3);
        for (hit, id) in hits.iter().zip(live) {
            let entity = hit.entity.as_ref().unwrap();
            assert_eq!(&entity.id, id);
            assert_eq!(entity, &load_entity(&pool, id).await.expect("load"));
        }
    }
}
