use std::str::FromStr;
use std::sync::Once;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{ConnectOptions, Sqlite, SqlitePool, Transaction};

use crate::structures::{self, fold_name, StructureDef};

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
            libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut libsqlite3_sys::sqlite3,
                    *mut *mut std::ffi::c_char,
                    *const libsqlite3_sys::sqlite3_api_routines,
                ) -> std::ffi::c_int,
            >(
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
        .busy_timeout(Duration::from_secs(5))
        // A slow statement is a warning, logged with its SQL (no bound values).
        .log_slow_statements(log::LevelFilter::Warn, Duration::from_millis(100));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

/// Run the migrations, then bring every folded name up to `fold_name` (see
/// `refold_names`).
pub(crate) async fn migrate(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::migrate!().run(pool).await?;
    refold_names(pool).await
}

/// Set each entity's `name_key`, and its name key's `entity_keys` value if its
/// structure declares one, to `fold_name` of its name wherever it differs. SQL
/// can't fold non-ASCII names, so this finishes the `name_key` migration's
/// backfill, and refolds the keys if `fold_name` ever changes. Once every key is
/// current it only reads. A name key two entities now share stays as it was,
/// with a warning: refolding mustn't merge them.
async fn refold_names(pool: &SqlitePool) -> sqlx::Result<()> {
    let rows = sqlx::query!(r#"SELECT id AS "id!", structure_type, name, name_key FROM entities"#)
        .fetch_all(pool)
        .await?;
    let stale: Vec<_> = rows
        .into_iter()
        .filter(|r| fold_name(&r.name) != r.name_key)
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    let mut tx = begin_write(pool).await?;
    for row in &stale {
        let key = fold_name(&row.name);
        sqlx::query!("UPDATE entities SET name_key = ? WHERE id = ?", key, row.id)
            .execute(&mut *tx)
            .await?;
        let Some(def) = structures::structure(&row.structure_type).and_then(StructureDef::name_key)
        else {
            continue;
        };
        let refolded = sqlx::query!(
            "UPDATE entity_keys SET value = ? WHERE entity_id = ? AND key = ?",
            key,
            row.id,
            def.key
        )
        .execute(&mut *tx)
        .await;
        match refolded {
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => tracing::warn!(
                entity_id = row.id,
                name = row.name,
                "another {} has this name once refolded; its key is left as it was",
                row.structure_type
            ),
            refolded => {
                refolded?;
            }
        }
    }
    tx.commit().await?;
    tracing::info!(count = stale.len(), "refolded names");
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use crate::test_support::{memory_pool, unmigrated_memory_pool};

    /// The migration before `name_key`.
    const BEFORE_NAME_KEY: i64 = 20_261_025_000_000;

    async fn insert(pool: &SqlitePool, id: &str, structure_type: &str, name: &str) {
        sqlx::query(
            "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
             VALUES (?, ?, ?, 0, 0)",
        )
        .bind(id)
        .bind(structure_type)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert entity");
    }

    async fn insert_name_key(pool: &SqlitePool, id: &str, value: &str) {
        sqlx::query(
            "INSERT INTO entity_keys (structure_type, key, value, entity_id)
             VALUES ('Tag', 'name', ?, ?)",
        )
        .bind(value)
        .bind(id)
        .execute(pool)
        .await
        .expect("insert key");
    }

    async fn name_key(pool: &SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT name_key FROM entities WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("name_key")
    }

    async fn tag_key(pool: &SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT value FROM entity_keys WHERE entity_id = ? AND key = 'name'")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("tag key")
    }

    // I-83: the migration and the refold after it give every existing name its
    // `fold_name` key, non-ASCII too, and refold a Tag's name key with it.
    #[tokio::test]
    async fn existing_names_get_folded_keys() {
        let pool = unmigrated_memory_pool().await;
        sqlx::migrate!()
            .run_to(BEFORE_NAME_KEY, &pool)
            .await
            .expect("migrate to before name_key");
        insert(&pool, "ship", "Note", " Ship ").await;
        insert(&pool, "korolev", "Note", "Сергей Королёв").await;
        // Tag keys were `str::to_lowercase`, which writes a final Σ as ς.
        insert(&pool, "odos", "Tag", "ΟΔΟΣ").await;
        insert_name_key(&pool, "odos", "οδος").await;

        super::migrate(&pool).await.expect("migrate");

        assert_eq!(name_key(&pool, "ship").await, "ship");
        assert_eq!(name_key(&pool, "korolev").await, "сергей королёв");
        assert_eq!(name_key(&pool, "odos").await, "οδοσ");
        assert_eq!(tag_key(&pool, "odos").await, "οδοσ");
    }

    // Two Tags whose keys refolding would make equal keep their keys: the
    // refold warns rather than fail the boot or merge them.
    #[tokio::test]
    async fn refolding_onto_a_taken_tag_key_keeps_it() {
        let pool = unmigrated_memory_pool().await;
        sqlx::migrate!()
            .run_to(BEFORE_NAME_KEY, &pool)
            .await
            .expect("migrate to before name_key");
        insert(&pool, "upper", "Tag", "ΟΔΟΣ").await;
        insert_name_key(&pool, "upper", "οδος").await;
        insert(&pool, "lower", "Tag", "οδοσ").await;
        insert_name_key(&pool, "lower", "οδοσ").await;

        super::migrate(&pool).await.expect("migrate");

        assert_eq!(tag_key(&pool, "upper").await, "οδος");
        assert_eq!(tag_key(&pool, "lower").await, "οδοσ");
        assert_eq!(name_key(&pool, "upper").await, "οδοσ");
    }

    // ADR 15's MatchNames matches a prefix of the folded key: a range on the
    // index, not a scan.
    #[tokio::test]
    async fn a_name_key_prefix_match_uses_the_index() {
        let pool = memory_pool().await;
        let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
            "EXPLAIN QUERY PLAN SELECT id FROM entities
             WHERE structure_type = 'Note' AND name_key >= 'mer' AND name_key < 'mes'",
        )
        .fetch_all(&pool)
        .await
        .expect("query plan");
        let detail = plan
            .iter()
            .map(|p| p.3.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            detail.contains(
                "USING INDEX entities_name_key (structure_type=? AND name_key>? AND name_key<?)"
            ),
            "{detail}"
        );
    }

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

    #[tokio::test]
    async fn relationships_cascades_from_either_end() {
        let pool = memory_pool().await;
        let keys: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT \"from\", \"table\", on_delete FROM pragma_foreign_key_list('relationships')
             ORDER BY \"from\"",
        )
        .fetch_all(&pool)
        .await
        .expect("relationships has foreign keys");
        let keys: Vec<(&str, &str, &str)> = keys
            .iter()
            .map(|(from, table, on_delete)| (from.as_str(), table.as_str(), on_delete.as_str()))
            .collect();
        assert_eq!(
            keys,
            [
                ("entity_id", "entities", "CASCADE"),
                ("target_id", "entities", "CASCADE"),
            ]
        );
        let indexed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_index_list('relationships') WHERE name = 'relationships_target'",
        )
        .fetch_one(&pool)
        .await
        .expect("index list");
        assert_eq!(indexed, 1);

        for id in ["a", "b", "c"] {
            sqlx::query(
                "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
                 VALUES (?, 'Note', ?, 0, 0)",
            )
            .bind(id)
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert entity");
        }
        for (entity_id, target_id) in [("a", "b"), ("b", "c")] {
            sqlx::query(
                "INSERT INTO relationships (entity_id, target_id, created_at) VALUES (?, ?, 0)",
            )
            .bind(entity_id)
            .bind(target_id)
            .execute(&pool)
            .await
            .expect("insert relationship");
        }
        let duplicate = sqlx::query(
            "INSERT INTO relationships (entity_id, target_id, created_at) VALUES ('a', 'b', 1)",
        )
        .execute(&pool)
        .await;
        assert!(
            duplicate.is_err(),
            "(entity_id, target_id) is the primary key"
        );

        let count = || async {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM relationships")
                .fetch_one(&pool)
                .await
                .expect("count")
        };
        assert_eq!(count().await, 2);

        // Deleting the target ('c') removes b -> c; deleting the source ('a')
        // removes a -> b.
        sqlx::query("DELETE FROM entities WHERE id = 'c'")
            .execute(&pool)
            .await
            .expect("delete target");
        assert_eq!(count().await, 1);
        sqlx::query("DELETE FROM entities WHERE id = 'a'")
            .execute(&pool)
            .await
            .expect("delete source");
        assert_eq!(count().await, 0);
    }
}
