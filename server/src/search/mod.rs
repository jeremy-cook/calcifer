//! The search index's storage. An entity is found by its name, the names of the
//! Tags its properties hold, and the plain text of its rich-text docs, where a
//! chip reads as its target's current name, not the label stored with it (I-87,
//! I-103). `services::search` queries what this writes.
//!
//! - [`text`] reads an entity as search sees it, for the FTS row and the
//!   embedding chunks alike.
//! - [`fts`] keeps the `entity_fts` rows: the one reindex every write path
//!   shares, and the boot's fill of a rebuilt table.

pub mod fts;
pub mod text;
