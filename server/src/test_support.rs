//! Shared helpers for service-level tests: an in-memory, fully-migrated SQLite
//! pool and services built on it the same way `main.rs` builds them.

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::embed::EmbedHandle;
use crate::proto::CreateEntityRequest;
use crate::services::entity::EntityService;
use crate::services::richtext::RichTextService;
use crate::watch::WatchHub;

/// A fresh in-memory database with every migration applied. See
/// `unmigrated_memory_pool`.
pub(crate) async fn memory_pool() -> SqlitePool {
    let pool = unmigrated_memory_pool().await;
    sqlx::migrate!().run(&pool).await.expect("run migrations");
    pool
}

/// A fresh in-memory database with no migrations applied, for tests that run
/// migrations themselves. Each `:memory:` connection is its own database, so the
/// pool is pinned to exactly one connection that never expires. Foreign keys are
/// on, as in production.
pub(crate) async fn unmigrated_memory_pool() -> SqlitePool {
    // The vec migration creates a vec0 table, so sqlite-vec must be registered
    // before the connection opens (same as `db::connect`).
    crate::db::register_sqlite_vec();

    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("valid in-memory url")
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .expect("open in-memory pool")
}

/// A fresh file-backed database opened by `db::connect`, as `main.rs` opens it
/// (WAL, busy_timeout, several connections), for tests that race two writers:
/// `memory_pool` has a single connection, so nothing can race on it. The file
/// and its WAL sidecars are removed when dropped, even if the test panics.
pub(crate) struct ScratchDb {
    pub pool: SqlitePool,
    path: std::path::PathBuf,
}

impl ScratchDb {
    pub async fn new() -> Self {
        let path = std::env::temp_dir().join(format!("calcifer-race-{}.db", uuid::Uuid::new_v4()));
        let url = format!("sqlite://{}", path.display());
        let pool = crate::db::connect(&url).await.expect("open file pool");
        Self { pool, path }
    }
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut path = self.path.clone().into_os_string();
            path.push(suffix);
            let _ = std::fs::remove_file(path);
        }
    }
}

/// An `EntityService` on `pool`, wired as in `main.rs` but with embeddings
/// disabled (the lightest legitimate handle: no model load or download).
pub(crate) fn entity_service(pool: SqlitePool) -> EntityService {
    EntityService::new(pool, WatchHub::new(), EmbedHandle::disabled())
}

/// A `RichTextService` on `pool` publishing to `hub`, wired as in `main.rs`
/// but with embeddings disabled. Pass the hub a test subscribes to.
pub(crate) fn richtext_service(pool: SqlitePool, hub: WatchHub) -> RichTextService {
    RichTextService::new(pool, hub, EmbedHandle::disabled())
}

/// A Create request for a Note named `name`: intent only, the server builds it.
pub(crate) fn note(name: &str) -> CreateEntityRequest {
    create_request("Note", name)
}

/// A Create request for a Todo named `name`; the server adds the select defaults.
pub(crate) fn todo(name: &str) -> CreateEntityRequest {
    create_request("Todo", name)
}

/// A Create request for a Tag named `name`.
pub(crate) fn tag(name: &str) -> CreateEntityRequest {
    create_request("Tag", name)
}

fn create_request(structure_type: &str, name: &str) -> CreateEntityRequest {
    CreateEntityRequest {
        structure_type: structure_type.to_string(),
        name: Some(name.to_string()),
        properties: Default::default(),
    }
}
