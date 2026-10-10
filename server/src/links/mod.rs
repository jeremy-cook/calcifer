//! The link graph's storage (ADR 3). Links come from two sources, rich-text docs
//! (read by `tiptap::references`) and relation property values, and both end in
//! the same scoped replace.
//!
//! - [`store`] writes the `links` table and resolves mention targets.
//! - [`relations`] turns relation property values into link rows.

pub mod relations;
pub mod store;
