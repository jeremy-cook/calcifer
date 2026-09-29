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
}
