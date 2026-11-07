//! `EntityService`: the entity RPCs, split by what each part does.
//!
//! - [`rpc`] is the `EntityService` trait impl: one handler per RPC.
//! - [`write`], [`resolve`] and [`relations`] are the write paths the handlers share:
//!   persisting and replacing, get-or-create by name or date, and relation and
//!   relationship edits.
//! - [`load`] hydrates entities. The search rows are `crate::search_index`'s.
//! - [`construct`] builds a new entity; [`validate`] checks property values and
//!   names; [`keys`] keeps the natural-key rows; [`lookup`] reads the structure
//!   registry and an entity's facets.
//! - [`watch`] is the `WatchEntities` stream.

use sqlx::SqlitePool;

use crate::embed::EmbedHandle;
use crate::watch::WatchHub;

mod construct;
mod keys;
mod load;
mod lookup;
mod relations;
mod resolve;
mod rpc;
#[cfg(test)]
mod tests;
mod validate;
mod watch;
mod write;

pub(crate) use load::{load_entities_by_id, load_entity, ts_from_millis};
pub(crate) use validate::{check_name_chars, reject_reserved_property_id};

/// The reserved link scope a relationship's links are derived under (ADR 12, rules 8
/// and 9). No property can use it: ids starting with `@` are refused.
const RELATED_SCOPE: &str = "@related";

pub struct EntityService {
    pool: SqlitePool,
    hub: WatchHub,
    embed: EmbedHandle,
}

impl EntityService {
    pub fn new(pool: SqlitePool, hub: WatchHub, embed: EmbedHandle) -> Self {
        Self { pool, hub, embed }
    }

    /// Queue for embedding the entities a committed write's `reindex` changed.
    fn embed_all(&self, ids: Vec<String>) {
        for id in ids {
            self.embed.enqueue(id);
        }
    }
}
