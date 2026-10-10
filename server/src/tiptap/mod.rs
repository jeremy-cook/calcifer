//! Reading TipTap (ProseMirror) JSON docs: the node types and attributes the server
//! understands are specified in docs/reference/richtext-doc.md.
//!
//! - [`references`] extracts what a doc points at: mention targets and date chips.
//! - [`text`] flattens a doc to plain text, the one rule for FTS and embedding chunks.
//!
//! This is the only implementation of either; no client has a copy.

use serde_json::Value;

use crate::error::AppError;

pub mod references;
pub mod text;

/// Node types that point at an entity: `@` mentions and `#` tags.
const MENTION_TYPES: [&str; 2] = ["mention", "hashtag"];

/// Parse a doc, or `None` for an empty / blank one (a document never saved).
fn parse(doc_json: &str) -> Result<Option<Value>, AppError> {
    if doc_json.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(doc_json)
        .map(Some)
        .map_err(|e| AppError::Invalid(format!("invalid richtext doc json: {e}")))
}
