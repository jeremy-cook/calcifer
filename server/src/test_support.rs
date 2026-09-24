//! Shared helpers for service-level tests: an in-memory, fully-migrated SQLite
//! pool and services built on it the same way `main.rs` builds them.

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::embed::EmbedHandle;
use crate::proto::{property_value, Entity, Property, PropertyValue, RichTextRef};
use crate::services::entity::EntityService;
use crate::watch::WatchHub;

/// A fresh in-memory database with every migration applied. Each `:memory:`
/// connection is its own database, so the pool is pinned to exactly one
/// connection that never expires. Foreign keys are on, as in production.
pub(crate) async fn memory_pool() -> SqlitePool {
    // The vec migration creates a vec0 table, so sqlite-vec must be registered
    // before the connection opens (same as `db::connect`).
    crate::db::register_sqlite_vec();

    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("valid in-memory url")
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .expect("open in-memory pool");

    sqlx::migrate!().run(&pool).await.expect("run migrations");
    pool
}

/// An `EntityService` on `pool`, wired as in `main.rs` but with embeddings
/// disabled (the lightest legitimate handle: no model load or download).
pub(crate) fn entity_service(pool: SqlitePool) -> EntityService {
    EntityService::new(pool, WatchHub::new(), EmbedHandle::disabled())
}

/// A new Note entity carrying its `content` richtext pointer, as the FE builds it.
pub(crate) fn note(name: &str) -> Entity {
    let id = uuid::Uuid::new_v4().to_string();
    Entity {
        properties: vec![Property {
            id: "content".to_string(),
            value: Some(PropertyValue {
                value: Some(property_value::Value::Richtext(RichTextRef {
                    entity_id: id.clone(),
                    property_id: "content".to_string(),
                })),
            }),
        }],
        id,
        structure_type: "Note".to_string(),
        name: name.to_string(),
        links: vec![],
        referenced_dates: vec![],
        created_at: None,
        updated_at: None,
    }
}

/// A new Todo entity as the FE builds it: `content` plus the table's select
/// defaults (`status = open`, `priority = none`), no tags.
pub(crate) fn todo(name: &str) -> Entity {
    let mut entity = note(name);
    entity.structure_type = "Todo".to_string();
    for (id, default) in crate::structures::select_defaults("Todo") {
        entity.properties.push(Property {
            id: id.to_string(),
            value: Some(PropertyValue {
                value: Some(property_value::Value::Select(default.to_string())),
            }),
        });
    }
    entity
}
