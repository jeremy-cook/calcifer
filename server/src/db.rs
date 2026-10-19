use std::str::FromStr;
use std::sync::Once;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};

static VEC_INIT: Once = Once::new();

/// Register the `sqlite-vec` extension as a SQLite auto-extension so every
/// connection the pool opens (now and later) has `vec0` virtual tables and the
/// `vec_*` functions available. This MUST run before any connection is opened.
///
/// `sqlite3_auto_extension` is exported by libsqlite3-sys (the same native
/// sqlite3 that sqlx-sqlite links), and `sqlite3_vec_init` by the sqlite-vec
/// crate; we hand the latter to the former as an entry-point callback. Guarded by
/// a `Once` so re-entry (e.g. tests) doesn't register it twice.
pub(crate) fn register_sqlite_vec() {
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

/// Begin a write transaction. IMMEDIATE takes the write lock before the first
/// read. A DEFERRED transaction that has already read can't upgrade to a writer
/// once another connection commits: SQLite returns SQLITE_BUSY at once, ignoring
/// busy_timeout, and the caller fails as Internal (I-37, I-40). With IMMEDIATE a
/// contending writer waits on busy_timeout instead, then reads current data.
pub async fn begin_write(pool: &SqlitePool) -> sqlx::Result<Transaction<'static, Sqlite>> {
    pool.begin_with("BEGIN IMMEDIATE").await
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

#[cfg(test)]
mod tests {
    use crate::test_support::memory_pool;

    #[tokio::test]
    async fn entity_facets_cascades_from_entities() {
        let pool = memory_pool().await;
        let (table, on_delete): (String, String) = sqlx::query_as(
            "SELECT \"table\", on_delete FROM pragma_foreign_key_list('entity_facets')",
        )
        .fetch_one(&pool)
        .await
        .expect("entity_facets has a foreign key");
        assert_eq!(
            (table.as_str(), on_delete.as_str()),
            ("entities", "CASCADE")
        );

        sqlx::query(
            "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
             VALUES ('e1', 'Note', 'Ship', 0, 0)",
        )
        .execute(&pool)
        .await
        .expect("insert entity");
        sqlx::query("INSERT INTO entity_facets (entity_id, facet) VALUES ('e1', 'todo')")
            .execute(&pool)
            .await
            .expect("insert facet");
        let duplicate =
            sqlx::query("INSERT INTO entity_facets (entity_id, facet) VALUES ('e1', 'todo')")
                .execute(&pool)
                .await;
        assert!(duplicate.is_err(), "(entity_id, facet) is the primary key");

        sqlx::query("DELETE FROM entities WHERE id = 'e1'")
            .execute(&pool)
            .await
            .expect("delete entity");
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity_facets")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(left, 0);
    }
}
