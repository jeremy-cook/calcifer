//! Background embed worker: keeps `chunks` + `entity_vec` in sync with content,
//! off the write hot-path.
//!
//! Write paths (`RichTextService.PutRichText`, `EntityService` create/update) call
//! `EmbedHandle::enqueue(entity_id)` after their transaction commits. The handle
//! pushes the id onto an unbounded channel and returns instantly — the request
//! never waits on chunking or the ONNX model. A single spawned task drains the
//! channel, and for each id: reads the entity's richtext docs, re-chunks +
//! re-embeds, and delete-then-inserts that entity's `chunks`/`entity_vec` rows
//! (wholesale per entity, mirroring how `entity_fts` is owned by the writers).
//!
//! `EmbedHandle` also exposes `embed_query` so `SearchService.Search` (semantic
//! and hybrid modes) can embed the search string through the same model. When
//! the embedder failed to initialise (e.g. no network on a cold model cache) the
//! handle is inert: `enqueue` is a no-op and `embed_query` returns None, so
//! retrieval degrades to lexical-only and the rest of the server is unaffected.

use std::sync::Arc;
use std::time::Instant;

use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
use opentelemetry::trace::{SpanContext, TraceContextExt};
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::attribute::ERROR_TYPE;
use sqlx::SqlitePool;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::embed::chunk::chunk_doc;
use crate::embed::provider::{Embedder, LocalEmbedder, EMBED_DIM};
use crate::error::AppError;

/// Cheap-to-clone handle the write paths and SearchService hold. `inner` is None
/// when the embedder could not initialise — every method then degrades safely.
#[derive(Clone)]
pub struct EmbedHandle {
    inner: Option<Arc<HandleInner>>,
}

struct HandleInner {
    tx: UnboundedSender<Job>,
    embedder: Arc<dyn Embedder>,
    queued: UpDownCounter<i64>,
}

/// An entity to embed, and the span of the write that queued it, which the
/// job's span links back to.
struct Job {
    entity_id: String,
    queued_by: SpanContext,
}

/// `calcifer.embed.*`: the queue's depth, and per job its duration, its
/// failures and the chunks it wrote.
struct Instruments {
    duration: Histogram<f64>,
    failures: Counter<u64>,
    chunks: Counter<u64>,
}

fn queue_depth(meter: &Meter) -> UpDownCounter<i64> {
    meter
        .i64_up_down_counter("calcifer.embed.queue.depth")
        .with_description("Entities waiting to be embedded")
        .with_unit("{entity}")
        .build()
}

impl Instruments {
    fn new(meter: &Meter) -> Self {
        Self {
            duration: meter
                .f64_histogram("calcifer.embed.duration")
                .with_description("Time to chunk, embed and store one entity")
                .with_unit("s")
                .build(),
            failures: meter
                .u64_counter("calcifer.embed.failures")
                .with_description("Embed jobs that failed")
                .with_unit("{entity}")
                .build(),
            chunks: meter
                .u64_counter("calcifer.embed.chunks")
                .with_description("Chunks embedded and stored")
                .with_unit("{chunk}")
                .build(),
        }
    }
}

impl EmbedHandle {
    /// Disabled handle: enqueue is a no-op, query embedding yields None.
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// A handle that embeds queries with `embedder` and runs no worker, so
    /// `enqueue` is a no-op. For tests that search against hand-written vectors.
    #[cfg(test)]
    pub(crate) fn with_embedder(embedder: Arc<dyn Embedder>) -> Self {
        use opentelemetry::metrics::{MeterProvider, NoopMeterProvider};

        let (tx, _rx) = mpsc::unbounded_channel::<Job>();
        Self {
            inner: Some(Arc::new(HandleInner {
                tx,
                embedder,
                queued: queue_depth(&NoopMeterProvider::new().meter("test")),
            })),
        }
    }

    /// Queue an entity for (re)embedding. Returns immediately; never blocks the
    /// caller on model work. Silently no-ops when embeddings are disabled.
    pub fn enqueue(&self, entity_id: impl Into<String>) {
        if let Some(inner) = &self.inner {
            let job = Job {
                entity_id: entity_id.into(),
                queued_by: Span::current().context().span().span_context().clone(),
            };
            // Send only fails if the worker is gone; nothing useful to do then.
            if inner.tx.send(job).is_ok() {
                inner.queued.add(1, &[]);
            }
        }
    }

    /// Embed a query string for vector KNN. Returns None when embeddings are
    /// disabled, so callers fall back to lexical search. Runs the CPU-bound model
    /// on a blocking thread to avoid stalling the async runtime.
    #[tracing::instrument(skip_all)]
    pub async fn embed_query(&self, query: &str) -> Result<Option<Vec<f32>>, AppError> {
        let Some(inner) = &self.inner else {
            return Ok(None);
        };
        let embedder = inner.embedder.clone();
        let query = query.to_string();
        let vec = tokio::task::spawn_blocking(move || embedder.embed_query(&query))
            .await
            .map_err(|e| AppError::Invalid(format!("embed task join error: {e}")))??;
        Ok(Some(vec))
    }
}

/// Build the embedder and spawn the drain loop, recording `calcifer.embed.*` on
/// `meter`. On embedder-init failure (logged) returns a disabled handle so the
/// server still boots and serves lexical search.
pub fn spawn(pool: SqlitePool, meter: &Meter) -> EmbedHandle {
    let embedder = match LocalEmbedder::try_new() {
        Ok(e) => Arc::new(e) as Arc<dyn Embedder>,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "semantic embeddings disabled; semantic and hybrid search fall back to lexical"
            );
            return EmbedHandle::disabled();
        }
    };

    let (tx, rx) = mpsc::unbounded_channel::<Job>();
    let queued = queue_depth(meter);
    tokio::spawn(run(
        pool,
        rx,
        embedder.clone(),
        queued.clone(),
        Instruments::new(meter),
    ));
    tracing::info!(
        model = "all-MiniLM-L6-v2",
        dim = EMBED_DIM,
        "semantic embed worker started"
    );

    EmbedHandle {
        inner: Some(Arc::new(HandleInner {
            tx,
            embedder,
            queued,
        })),
    }
}

/// Drain loop: one entity at a time. A failure for one entity is logged and
/// skipped so it never wedges the queue.
async fn run(
    pool: SqlitePool,
    mut rx: UnboundedReceiver<Job>,
    embedder: Arc<dyn Embedder>,
    queued: UpDownCounter<i64>,
    instruments: Instruments,
) {
    while let Some(job) = rx.recv().await {
        queued.add(-1, &[]);
        // A trace of its own, linked to the write: the request has long ended.
        let span = tracing::info_span!("embed", entity_id = %job.entity_id);
        if job.queued_by.is_valid() {
            span.add_link(job.queued_by);
        }
        let started = Instant::now();
        let result = reembed_entity(&pool, &embedder, &job.entity_id)
            .instrument(span.clone())
            .await;
        let elapsed = started.elapsed().as_secs_f64();
        match result {
            Ok(chunks) => {
                instruments.duration.record(elapsed, &[]);
                instruments.chunks.add(chunks as u64, &[]);
            }
            Err(e) => {
                let error_type = [KeyValue::new(ERROR_TYPE, "embed_failed")];
                instruments.duration.record(elapsed, &error_type);
                instruments.failures.add(1, &[]);
                span.in_scope(|| {
                    tracing::warn!(entity_id = %job.entity_id, error = %e, "embed failed");
                });
            }
        }
    }
}

/// Re-chunk and re-embed one entity, replacing its `chunks`/`entity_vec` rows.
/// Returns how many chunks it stored.
async fn reembed_entity(
    pool: &SqlitePool,
    embedder: &Arc<dyn Embedder>,
    entity_id: &str,
) -> Result<usize, AppError> {
    // Concatenate the entity's richtext docs (it may have several properties),
    // then chunk the combined content. Runtime query: plain table, but keep the
    // whole module consistent on the unchecked API.
    let docs: Vec<String> = sqlx::query_scalar("SELECT doc FROM richtext WHERE entity_id = ?")
        .bind(entity_id)
        .fetch_all(pool)
        .await?;

    let mut chunks: Vec<String> = Vec::new();
    for doc in &docs {
        chunks.extend(chunk_doc(doc));
    }

    // Embed off the async runtime (CPU-bound ONNX). Clone the work into the
    // blocking thread; an empty chunk list skips the model entirely.
    let embeddings = if chunks.is_empty() {
        vec![]
    } else {
        let embedder = embedder.clone();
        let to_embed = chunks.clone();
        tokio::task::spawn_blocking(move || embedder.embed(&to_embed))
            .await
            .map_err(|e| AppError::Invalid(format!("embed task join error: {e}")))??
    };

    let mut tx = crate::db::begin_write(pool).await?;

    // Drop the entity's prior rows; entity_vec keys on the same ids as chunks.
    let old_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM chunks WHERE entity_id = ?")
        .bind(entity_id)
        .fetch_all(&mut *tx)
        .await?;
    for id in &old_ids {
        sqlx::query("DELETE FROM entity_vec WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM chunks WHERE entity_id = ?")
        .bind(entity_id)
        .execute(&mut *tx)
        .await?;

    // Insert fresh chunk rows + their embeddings, sharing the chunk rowid as the
    // entity_vec key. sqlite-vec wants the f32 vector as a little-endian blob.
    for (idx, (text, embedding)) in chunks.iter().zip(embeddings.iter()).enumerate() {
        // Guard the pinned dimension: a model emitting a different width would
        // silently corrupt the float[384] vec0 column, so reject it loudly.
        if embedding.len() != EMBED_DIM {
            return Err(AppError::Invalid(format!(
                "embedding dim {} != pinned {EMBED_DIM}",
                embedding.len()
            )));
        }
        let inserted = sqlx::query_scalar(
            "INSERT INTO chunks (entity_id, chunk_index, text) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(entity_id)
        .bind(idx as i64)
        .bind(text)
        .fetch_one(&mut *tx)
        .await;
        let chunk_id: i64 = match inserted {
            Ok(id) => id,
            // `chunks` references `entities`: the entity was deleted while its
            // docs were embedding. Nothing to keep, so roll back the rows this
            // tx already wrote and skip it.
            Err(sqlx::Error::Database(db)) if db.is_foreign_key_violation() => {
                tx.rollback().await?;
                return Ok(0);
            }
            Err(e) => return Err(e.into()),
        };

        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query("INSERT INTO entity_vec (id, embedding) VALUES (?, ?)")
            .bind(chunk_id)
            .bind(&blob[..])
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(chunks.len())
}

/// Encode a query embedding as the little-endian f32 blob sqlite-vec's MATCH
/// expects. Shared with `SearchService.Search`.
pub fn vector_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::memory_pool;

    /// Deletes `entity_id` while embedding, as a DeleteEntity that commits
    /// between the worker's read and its write would.
    struct DeletingEmbedder {
        pool: SqlitePool,
        entity_id: String,
    }

    impl Embedder for DeletingEmbedder {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
            tokio::runtime::Handle::current()
                .block_on(
                    sqlx::query("DELETE FROM entities WHERE id = ?")
                        .bind(&self.entity_id)
                        .execute(&self.pool),
                )
                .expect("delete entity");
            Ok(texts.iter().map(|_| vec![0.0; EMBED_DIM]).collect())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_entity_deleted_while_embedding_is_skipped() {
        let pool = memory_pool().await;
        sqlx::query(
            "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
             VALUES ('n', 'Note', 'n', 0, 0)",
        )
        .execute(&pool)
        .await
        .expect("insert entity");
        let doc = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"one"}]},{"type":"paragraph","content":[{"type":"text","text":"two"}]}]}"#;
        sqlx::query(
            "INSERT INTO richtext (entity_id, property_id, doc, updated_at)
             VALUES ('n', 'content', ?, 0)",
        )
        .bind(doc)
        .execute(&pool)
        .await
        .expect("insert richtext");
        let embedder: Arc<dyn Embedder> = Arc::new(DeletingEmbedder {
            pool: pool.clone(),
            entity_id: "n".to_string(),
        });

        reembed_entity(&pool, &embedder, "n")
            .await
            .expect("a deleted entity isn't an error");

        for (table, sql) in [
            ("entities", "SELECT COUNT(*) FROM entities"),
            ("chunks", "SELECT COUNT(*) FROM chunks"),
            ("entity_vec", "SELECT COUNT(*) FROM entity_vec"),
        ] {
            let rows: i64 = sqlx::query_scalar(sql).fetch_one(&pool).await.expect(table);
            assert_eq!(rows, 0, "{table}");
        }
    }
}
