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

use sqlx::SqlitePool;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

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
    tx: UnboundedSender<String>,
    embedder: Arc<dyn Embedder>,
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
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        Self {
            inner: Some(Arc::new(HandleInner { tx, embedder })),
        }
    }

    /// Queue an entity for (re)embedding. Returns immediately; never blocks the
    /// caller on model work. Silently no-ops when embeddings are disabled.
    pub fn enqueue(&self, entity_id: impl Into<String>) {
        if let Some(inner) = &self.inner {
            // Send only fails if the worker is gone; nothing useful to do then.
            let _ = inner.tx.send(entity_id.into());
        }
    }

    /// Embed a query string for vector KNN. Returns None when embeddings are
    /// disabled, so callers fall back to lexical search. Runs the CPU-bound model
    /// on a blocking thread to avoid stalling the async runtime.
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

/// Build the embedder and spawn the drain loop. On embedder-init failure (logged)
/// returns a disabled handle so the server still boots and serves lexical search.
pub fn spawn(pool: SqlitePool) -> EmbedHandle {
    let embedder = match LocalEmbedder::try_new() {
        Ok(e) => Arc::new(e) as Arc<dyn Embedder>,
        Err(e) => {
            tracing::warn!(
                "semantic embeddings disabled: {e}. Semantic and hybrid search fall back to lexical."
            );
            return EmbedHandle::disabled();
        }
    };

    let (tx, rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(run(pool, rx, embedder.clone()));
    tracing::info!("semantic embed worker started (local all-MiniLM-L6-v2, 384-d)");

    EmbedHandle {
        inner: Some(Arc::new(HandleInner { tx, embedder })),
    }
}

/// Drain loop: one entity at a time. A failure for one entity is logged and
/// skipped so it never wedges the queue.
async fn run(pool: SqlitePool, mut rx: UnboundedReceiver<String>, embedder: Arc<dyn Embedder>) {
    while let Some(entity_id) = rx.recv().await {
        if let Err(e) = reembed_entity(&pool, &embedder, &entity_id).await {
            tracing::warn!("embed failed for entity {entity_id}: {e}");
        }
    }
}

/// Re-chunk and re-embed one entity, replacing its `chunks`/`entity_vec` rows.
async fn reembed_entity(
    pool: &SqlitePool,
    embedder: &Arc<dyn Embedder>,
    entity_id: &str,
) -> Result<(), AppError> {
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

    let mut tx = pool.begin().await?;

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
        let chunk_id: i64 = sqlx::query_scalar(
            "INSERT INTO chunks (entity_id, chunk_index, text) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(entity_id)
        .bind(idx as i64)
        .bind(text)
        .fetch_one(&mut *tx)
        .await?;

        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query("INSERT INTO entity_vec (id, embedding) VALUES (?, ?)")
            .bind(chunk_id)
            .bind(&blob[..])
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Encode a query embedding as the little-endian f32 blob sqlite-vec's MATCH
/// expects. Shared with `SearchService.Search`.
pub fn vector_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}
