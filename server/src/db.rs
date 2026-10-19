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
    use crate::proto::RichTextRef;
    use crate::structures::{PropertyKind, STRUCTURES};
    use crate::test_support::unmigrated_memory_pool;

    /// The migration that deletes stored rich-text property values (I-22), and
    /// the one just before it.
    const DROP_RICHTEXT_VALUES: i64 = 20260926000000;
    const BEFORE_DROP_RICHTEXT_VALUES: i64 = 20260614000000;

    /// The migration that moves natural keys into `entity_keys` (ADR 10).
    const ENTITY_KEYS: i64 = 20261018000000;

    /// The migration that drops `chunks_entity`, and the one after it that makes
    /// `richtext` and `chunks` cascade from `entities`.
    const DROP_CHUNKS_ENTITY: i64 = 20261019000000;
    const CASCADE_RICHTEXT_CHUNKS: i64 = 20261019010000;

    /// The migration that addresses `entity_fts` rows by a mapped rowid.
    const ENTITY_FTS_ROWIDS: i64 = 20261019020000;

    /// The `(structure_type, property_id)` pairs the migration lists by hand.
    const MIGRATION_RICHTEXT_PAIRS: [(&str, &str); 3] = [
        ("Note", "content"),
        ("DailyNote", "content"),
        ("Todo", "content"),
    ];

    /// An old-style stored value: a `PropertyValue` whose (now reserved) field 6
    /// holds a `RichTextRef`, encoded by hand since the case is gone.
    fn old_richtext_blob(entity_id: &str, property_id: &str) -> Vec<u8> {
        let inner = prost::Message::encode_to_vec(&RichTextRef {
            entity_id: entity_id.to_string(),
            property_id: property_id.to_string(),
        });
        let mut blob = vec![(6 << 3) | 2, inner.len() as u8];
        blob.extend(inner);
        blob
    }

    #[test]
    fn drop_richtext_migration_lists_every_declared_richtext_property() {
        let declared: Vec<(&str, &str)> = STRUCTURES
            .iter()
            .flat_map(|s| {
                s.properties
                    .iter()
                    .filter(|p| p.kind == PropertyKind::Richtext)
                    .map(move |p| (s.type_, p.id))
            })
            .collect();
        assert_eq!(declared, MIGRATION_RICHTEXT_PAIRS);
    }

    #[tokio::test]
    async fn drop_richtext_migration_deletes_exactly_the_richtext_rows() {
        let pool = unmigrated_memory_pool().await;
        let migrator = sqlx::migrate!();
        assert!(migrator.version_exists(DROP_RICHTEXT_VALUES));
        migrator
            .run_to(BEFORE_DROP_RICHTEXT_VALUES, &pool)
            .await
            .expect("migrate to before the drop");

        // Old-style rows: a rich-text ref on every declared rich-text property,
        // plus rows the migration must keep: other properties of the same
        // entities, and `content` on structures that don't declare it rich text.
        let entities = [
            ("n", "Note"),
            ("d", "DailyNote"),
            ("t", "Todo"),
            ("g", "Tag"),
            ("x", "Custom"),
        ];
        for (id, structure_type) in entities {
            sqlx::query(
                "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
                 VALUES (?, ?, ?, 0, 0)",
            )
            .bind(id)
            .bind(structure_type)
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert entity");
        }
        let rows = [
            ("n", "content"),
            ("n", "mood"),
            ("d", "content"),
            ("d", "date"),
            ("t", "content"),
            ("t", "status"),
            ("g", "content"),
            ("x", "content"),
        ];
        for (entity_id, property_id) in rows {
            sqlx::query(
                "INSERT INTO properties (entity_id, property_id, value_blob) VALUES (?, ?, ?)",
            )
            .bind(entity_id)
            .bind(property_id)
            .bind(old_richtext_blob(entity_id, property_id))
            .execute(&pool)
            .await
            .expect("insert property");
        }
        sqlx::query(
            "INSERT INTO richtext (entity_id, property_id, doc, updated_at)
             VALUES ('n', 'content', '{}', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert richtext");

        migrator.run(&pool).await.expect("run the rest");

        let remaining: Vec<(String, String)> = sqlx::query_as(
            "SELECT entity_id, property_id FROM properties ORDER BY entity_id, property_id",
        )
        .fetch_all(&pool)
        .await
        .expect("remaining rows");
        let remaining: Vec<(&str, &str)> = remaining
            .iter()
            .map(|(e, p)| (e.as_str(), p.as_str()))
            .collect();
        assert_eq!(
            remaining,
            [
                ("d", "date"),
                ("g", "content"),
                ("n", "mood"),
                ("t", "status"),
                ("x", "content"),
            ]
        );
        let docs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM richtext")
            .fetch_one(&pool)
            .await
            .expect("count richtext");
        assert_eq!(docs, 1);
    }

    #[tokio::test]
    async fn entity_keys_migration_backfills_dates_and_tag_names() {
        let pool = unmigrated_memory_pool().await;
        let migrator = sqlx::migrate!();
        assert!(migrator.version_exists(ENTITY_KEYS));
        migrator
            .run_to(DROP_RICHTEXT_VALUES, &pool)
            .await
            .expect("migrate to before entity_keys");

        // A dated and an undated DailyNote, a Tag, and a Note, which has no key.
        let entities = [
            ("d", "DailyNote", "June 13, 2026", Some("2026-06-13")),
            ("u", "DailyNote", "Undated", None),
            ("g", "Tag", " Reading ", None),
            ("n", "Note", "Reading", None),
        ];
        for (id, structure_type, name, date_key) in entities {
            sqlx::query(
                "INSERT INTO entities (id, structure_type, name, date_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 0, 0)",
            )
            .bind(id)
            .bind(structure_type)
            .bind(name)
            .bind(date_key)
            .execute(&pool)
            .await
            .expect("insert entity");
        }

        migrator.run(&pool).await.expect("run the rest");

        let keys: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT structure_type, key, value, entity_id FROM entity_keys
             ORDER BY structure_type, key",
        )
        .fetch_all(&pool)
        .await
        .expect("key rows");
        let keys: Vec<(&str, &str, &str, &str)> = keys
            .iter()
            .map(|(s, k, v, e)| (s.as_str(), k.as_str(), v.as_str(), e.as_str()))
            .collect();
        assert_eq!(
            keys,
            [
                ("DailyNote", "date", "2026-06-13", "d"),
                ("Tag", "name", "reading", "g"),
            ]
        );
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('entities')")
                .fetch_all(&pool)
                .await
                .expect("entities columns");
        assert!(!columns.iter().any(|c| c == "date_key"), "{columns:?}");
    }

    #[tokio::test]
    async fn cascade_migration_keeps_rows_and_ids_and_drops_orphans() {
        let pool = unmigrated_memory_pool().await;
        let migrator = sqlx::migrate!();
        assert!(migrator.version_exists(CASCADE_RICHTEXT_CHUNKS));
        migrator
            .run_to(DROP_CHUNKS_ENTITY, &pool)
            .await
            .expect("migrate to before the cascade");

        sqlx::query(
            "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
             VALUES ('n', 'Note', 'n', 0, 0)",
        )
        .execute(&pool)
        .await
        .expect("insert entity");
        // 'gone' has rows but no entity: the orphans the migration drops.
        for (entity_id, doc) in [("n", "{\"kept\":1}"), ("gone", "{}")] {
            sqlx::query(
                "INSERT INTO richtext (entity_id, property_id, doc, updated_at)
                 VALUES (?, 'content', ?, 7)",
            )
            .bind(entity_id)
            .bind(doc)
            .execute(&pool)
            .await
            .expect("insert richtext");
        }
        for (id, entity_id, chunk_index) in [(40, "n", 0), (41, "gone", 0), (42, "n", 1)] {
            sqlx::query(
                "INSERT INTO chunks (id, entity_id, chunk_index, text) VALUES (?, ?, ?, 'text')",
            )
            .bind(id)
            .bind(entity_id)
            .bind(chunk_index)
            .execute(&pool)
            .await
            .expect("insert chunk");
            sqlx::query("INSERT INTO entity_vec (id, embedding) VALUES (?, ?)")
                .bind(id)
                .bind(crate::embed::vector_blob(
                    &[0.0; crate::embed::provider::EMBED_DIM],
                ))
                .execute(&pool)
                .await
                .expect("insert vector");
        }

        migrator.run(&pool).await.expect("run the rest");

        let docs: Vec<(String, String, String, i64)> =
            sqlx::query_as("SELECT entity_id, property_id, doc, updated_at FROM richtext")
                .fetch_all(&pool)
                .await
                .expect("richtext rows");
        assert_eq!(
            docs,
            [(
                "n".to_string(),
                "content".to_string(),
                "{\"kept\":1}".to_string(),
                7
            )]
        );
        let chunks: Vec<(i64, String, i64)> =
            sqlx::query_as("SELECT id, entity_id, chunk_index FROM chunks ORDER BY id")
                .fetch_all(&pool)
                .await
                .expect("chunk rows");
        assert_eq!(chunks, [(40, "n".to_string(), 0), (42, "n".to_string(), 1)]);
        let vectors: Vec<i64> = sqlx::query_scalar("SELECT id FROM entity_vec ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("vector rows");
        assert_eq!(vectors, [40, 42]);

        // The rebuilt tables cascade.
        sqlx::query("DELETE FROM entities WHERE id = 'n'")
            .execute(&pool)
            .await
            .expect("delete entity");
        let left: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM richtext) + (SELECT COUNT(*) FROM chunks)",
        )
        .fetch_one(&pool)
        .await
        .expect("count rows");
        assert_eq!(left, 0);
    }

    #[tokio::test]
    async fn fts_rowids_migration_carries_names_and_bodies_across() {
        let pool = unmigrated_memory_pool().await;
        let migrator = sqlx::migrate!();
        assert!(migrator.version_exists(ENTITY_FTS_ROWIDS));
        migrator
            .run_to(CASCADE_RICHTEXT_CHUNKS, &pool)
            .await
            .expect("migrate to before the rowids");

        for (id, created_at) in [("b", 1), ("a", 2), ("unindexed", 3)] {
            sqlx::query(
                "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
                 VALUES (?, 'Note', ?, ?, 0)",
            )
            .bind(id)
            .bind(format!("{id} name"))
            .bind(created_at)
            .execute(&pool)
            .await
            .expect("insert entity");
        }
        // Old-style rows, delete-then-insert by entity_id: 'a' was renamed
        // since it was indexed, 'unindexed' has no row, and 'gone' has no entity.
        for (entity_id, name, body) in [
            ("a", "a renamed", "alpha body"),
            ("gone", "gone", "stale"),
            ("b", "b name", "beta body"),
        ] {
            sqlx::query("INSERT INTO entity_fts (entity_id, name, body) VALUES (?, ?, ?)")
                .bind(entity_id)
                .bind(name)
                .bind(body)
                .execute(&pool)
                .await
                .expect("insert fts row");
        }

        migrator.run(&pool).await.expect("run the rest");

        let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
            "SELECT f.rowid, f.entity_id, f.name, f.body
             FROM entity_fts f JOIN entity_fts_rowids m
               ON m.fts_rowid = f.rowid AND m.entity_id = f.entity_id
             ORDER BY f.rowid",
        )
        .fetch_all(&pool)
        .await
        .expect("fts rows");
        let rows: Vec<(i64, &str, &str, &str)> = rows
            .iter()
            .map(|(r, e, n, b)| (*r, e.as_str(), n.as_str(), b.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (1, "b", "b name", "beta body"),
                (2, "a", "a renamed", "alpha body"),
                (3, "unindexed", "unindexed name", ""),
            ]
        );
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity_fts")
            .fetch_one(&pool)
            .await
            .expect("count fts");
        assert_eq!(total, 3);
        let hits: Vec<String> =
            sqlx::query_scalar("SELECT entity_id FROM entity_fts WHERE entity_fts MATCH 'alpha'")
                .fetch_all(&pool)
                .await
                .expect("match");
        assert_eq!(hits, ["a"]);
    }
}
