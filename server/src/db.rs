use std::str::FromStr;
use std::sync::Once;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

static VEC_INIT: Once = Once::new();

/// Register the `sqlite-vec` extension as a SQLite auto-extension so every
/// connection the pool opens (now and later) has `vec0` virtual tables and the
/// `vec_*` functions available. This MUST run before any connection is opened.
///
/// `sqlite3_auto_extension` is exported by libsqlite3-sys (the same native
/// sqlite3 that sqlx-sqlite links), and `sqlite3_vec_init` by the sqlite-vec
/// crate; we hand the latter to the former as an entry-point callback. Guarded by
/// a `Once` so re-entry (e.g. tests) doesn't register it twice.
fn register_sqlite_vec() {
    VEC_INIT.call_once(|| {
        // SAFETY: transmuting the C entry-point fn pointer into the auto-extension
        // callback shape is the documented registration path for sqlite-vec; both
        // come from the C ABI and are stable for the process lifetime.
        unsafe {
            libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

pub async fn connect(url: &str) -> sqlx::Result<SqlitePool> {
    register_sqlite_vec();

    // create_if_missing so a fresh checkout (or `rm calcifer.db`) self-bootstraps.
    // WAL + a busy_timeout let the background embed worker write concurrently with
    // request-handler transactions: WAL allows a writer alongside readers, and the
    // busy_timeout makes a contending writer wait for the lock instead of failing
    // fast with SQLITE_BUSY (which previously dropped embeddings under load).
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}
