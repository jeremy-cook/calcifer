//! Semantic-retrieval embedding subsystem (M8).
//!
//! - [`chunk`] splits TipTap docs into ~500-token text chunks.
//! - [`provider`] defines the `Embedder` trait + the local fastembed model.
//! - [`worker`] runs the background queue that keeps `chunks`/`entity_vec` in
//!   sync with content, and the `EmbedHandle` write paths and search share.

pub mod chunk;
pub mod provider;
pub mod worker;

pub use worker::{spawn, vector_blob, EmbedHandle};
