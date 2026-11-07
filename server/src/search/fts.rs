//! The `entity_fts` rows. [`reindex`] rewrites an entity's row from its
//! [`SearchText`] inside the writer's transaction, and when its name changed,
//! the rows of the entities that link to it, so their chips read the new name:
//! the one reindex every write path shares.

use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use super::text::SearchText;

/// The entity's `entity_fts` rowid, from `entity_fts_rowids`, assigning one if
/// it has none yet. FTS5 can't look up its UNINDEXED `entity_id` column, so
/// every `entity_fts` read and write goes through this rowid.
async fn fts_rowid(conn: &mut SqliteConnection, entity_id: &str) -> sqlx::Result<i64> {
    let existing = sqlx::query_scalar!(
        r#"SELECT fts_rowid AS "fts_rowid!" FROM entity_fts_rowids WHERE entity_id = ?"#,
        entity_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(rowid) = existing {
        return Ok(rowid);
    }
    sqlx::query_scalar!(
        r#"INSERT INTO entity_fts_rowids (entity_id) VALUES (?) RETURNING fts_rowid AS "fts_rowid!""#,
        entity_id
    )
    .fetch_one(&mut *conn)
    .await
}

/// What rewriting one entity's FTS row did.
struct Rewritten {
    changed: bool,
    renamed: bool,
}

/// Rewrite the entity's `entity_fts` row from its search text now, unless it
/// already holds that. None when there's no entity.
async fn rewrite_row(
    conn: &mut SqliteConnection,
    entity_id: &str,
) -> sqlx::Result<Option<Rewritten>> {
    let Some(text) = SearchText::load(conn, entity_id).await? else {
        return Ok(None);
    };
    let tags = text.tags.join(" ");
    let body = text.body();
    let rowid = fts_rowid(conn, entity_id).await?;
    // Runtime (non-macro) queries: sqlx's compile-time introspection chokes on
    // FTS5 virtual tables, so all `entity_fts` access is unchecked.
    let old = sqlx::query_as::<_, (String, String, String)>(
        "SELECT name, tags, body FROM entity_fts WHERE rowid = ?",
    )
    .bind(rowid)
    .fetch_optional(&mut *conn)
    .await?;
    if old.as_ref() == Some(&(text.name.clone(), tags.clone(), body.clone())) {
        return Ok(Some(Rewritten {
            changed: false,
            renamed: false,
        }));
    }
    let renamed = old.as_ref().is_none_or(|(name, _, _)| *name != text.name);
    let sql = if old.is_some() {
        "UPDATE entity_fts SET name = ?, tags = ?, body = ? WHERE rowid = ?"
    } else {
        "INSERT INTO entity_fts (name, tags, body, rowid, entity_id) VALUES (?, ?, ?, ?, ?)"
    };
    sqlx::query(sql)
        .bind(&text.name)
        .bind(&tags)
        .bind(&body)
        .bind(rowid)
        .bind(entity_id)
        .execute(&mut *conn)
        .await?;
    Ok(Some(Rewritten {
        changed: true,
        renamed,
    }))
}

/// Bring the entity's `entity_fts` row up to date, and when its name changed,
/// the rows of the entities that link to it. Call it inside the transaction of
/// any write that changes a name, a Tag a property holds, or a doc. Returns the
/// ids whose search text changed, the entity's first: queue them for embedding
/// once the transaction commits.
pub(crate) async fn reindex(
    tx: &mut Transaction<'_, Sqlite>,
    entity_id: &str,
) -> sqlx::Result<Vec<String>> {
    let Some(row) = rewrite_row(tx, entity_id).await? else {
        return Ok(vec![]);
    };
    if !row.changed {
        return Ok(vec![]);
    }
    let mut changed = vec![entity_id.to_string()];
    if !row.renamed {
        return Ok(changed);
    }
    let linkers = sqlx::query_scalar!(
        r#"SELECT DISTINCT entity_id AS "entity_id!" FROM links
           WHERE target_id = ? AND entity_id <> ? ORDER BY entity_id"#,
        entity_id,
        entity_id
    )
    .fetch_all(&mut **tx)
    .await?;
    for linker in linkers {
        if rewrite_row(tx, &linker).await?.is_some_and(|r| r.changed) {
            changed.push(linker);
        }
    }
    Ok(changed)
}

/// Index every entity with no `entity_fts` row: every entity, after a
/// migration rebuilds the table. Run at boot, after the migrations. Once every
/// entity has a row it only reads.
pub(crate) async fn index_missing(pool: &SqlitePool) -> sqlx::Result<()> {
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT e.id FROM entities e
         LEFT JOIN entity_fts_rowids r ON r.entity_id = e.id
         LEFT JOIN entity_fts f ON f.rowid = r.fts_rowid
         WHERE f.rowid IS NULL
         ORDER BY e.id",
    )
    .fetch_all(pool)
    .await?;
    if missing.is_empty() {
        return Ok(());
    }
    let mut tx = crate::db::begin_write(pool).await?;
    for id in &missing {
        rewrite_row(&mut tx, id).await?;
    }
    tx.commit().await?;
    tracing::info!(count = missing.len(), "indexed for search");
    Ok(())
}

/// Remove an entity's FTS row. Call inside the same tx as the entity delete,
/// before it: FTS5 rows don't cascade, and the delete cascades away the
/// `entity_fts_rowids` row that holds the rowid.
pub(crate) async fn fts_delete(
    tx: &mut Transaction<'_, Sqlite>,
    entity_id: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM entity_fts
         WHERE rowid = (SELECT fts_rowid FROM entity_fts_rowids WHERE entity_id = ?)",
    )
    .bind(entity_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::unmigrated_memory_pool;

    /// The migration before batch 3's, which rebuilds `entity_fts`.
    const BEFORE_SEARCH_TEXT: i64 = 20_261_104_000_000;

    // The rebuilt `entity_fts` starts empty; the boot indexes every entity.
    #[tokio::test]
    async fn the_boot_indexes_entities_with_no_search_row() {
        let pool = unmigrated_memory_pool().await;
        sqlx::migrate!()
            .run_to(BEFORE_SEARCH_TEXT, &pool)
            .await
            .expect("migrate to before batch 3");
        sqlx::query(
            r#"INSERT INTO entities (id, structure_type, name, created_at, updated_at)
               VALUES ('n', 'Note', 'Saturn V', 0, 0);
               INSERT INTO richtext (entity_id, property_id, doc, updated_at)
               VALUES ('n', 'content', '{"type":"doc","content":[{"type":"paragraph",
                 "content":[{"type":"text","text":"five engines"}]}]}', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("insert");

        crate::db::migrate(&pool).await.expect("migrate");

        let row: (String, String, String) =
            sqlx::query_as("SELECT name, tags, body FROM entity_fts WHERE entity_id = 'n'")
                .fetch_one(&pool)
                .await
                .expect("fts row");
        assert_eq!(
            row,
            ("Saturn V".into(), String::new(), "five engines".into())
        );
    }
}
