//! The `Embedder` trait + a local, in-process implementation.
//!
//! Nothing leaves the machine: `LocalEmbedder` runs the all-MiniLM-L6-v2 ONNX
//! model via fastembed, which downloads the (~90 MB) model to the fastembed cache
//! on first construction and then embeds entirely offline. The trait abstracts
//! that so the worker and `SearchService` depend only on "text in, 384-d vectors
//! out" — a remote or stub embedder could swap in without touching callers.

use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::error::AppError;

/// Embedding dimension produced by the local model. PINNED to match the
/// `entity_vec` migration (`float[384]`). all-MiniLM-L6-v2 emits 384-d vectors.
pub const EMBED_DIM: usize = 384;

/// Anything that turns batches of text into fixed-dimension vectors. Synchronous
/// CPU work; callers run it via `spawn_blocking` to stay off the async runtime.
pub trait Embedder: Send + Sync {
    /// Embed a batch of documents, preserving order. Each returned vector has
    /// length [`EMBED_DIM`].
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError>;

    /// Embed a single query string (convenience over `embed`).
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, AppError> {
        let mut out = self.embed(std::slice::from_ref(&text.to_string()))?;
        out.pop()
            .ok_or_else(|| AppError::Invalid("embedder returned no vector".into()))
    }
}

/// In-process fastembed (ONNX) embedder. The `TextEmbedding` session needs `&mut`
/// to run, so it lives behind a `Mutex`; embedding is CPU-bound and short, and
/// the worker is single-flighted, so contention is a non-issue.
pub struct LocalEmbedder {
    model: Mutex<TextEmbedding>,
}

impl LocalEmbedder {
    /// Construct the embedder, downloading the model on first use. Returns an
    /// error (rather than panicking) if the model can't be loaded — e.g. no
    /// network on a cold cache — so the caller can degrade to a no-op worker
    /// instead of taking the whole server down.
    pub fn try_new() -> Result<Self, AppError> {
        let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
            .map_err(|e| AppError::Invalid(format!("embedder init failed: {e}")))?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let mut model = self
            .model
            .lock()
            .map_err(|_| AppError::Invalid("embedder mutex poisoned".into()))?;
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        model
            .embed(refs, None)
            .map_err(|e| AppError::Invalid(format!("embedding failed: {e}")))
    }
}
