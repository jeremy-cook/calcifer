use crate::error::AppError;

/// The entity's `entity_fts` rowid, from `entity_fts_rowids`, assigning one if
/// it has none yet. FTS5 can't look up its UNINDEXED `entity_id` column, so
/// every `entity_fts` read and write goes through this rowid.
pub(super) async fn fts_rowid(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
) -> Result<i64, AppError> {
    let existing = sqlx::query_scalar!(
        r#"SELECT fts_rowid AS "fts_rowid!" FROM entity_fts_rowids WHERE entity_id = ?"#,
        entity_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(rowid) = existing {
        return Ok(rowid);
    }
    let rowid = sqlx::query_scalar!(
        r#"INSERT INTO entity_fts_rowids (entity_id) VALUES (?) RETURNING fts_rowid AS "fts_rowid!""#,
        entity_id
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(rowid)
}

/// Set one column of the entity's `entity_fts` row, inserting the row (other
/// column empty) if it has none. `update` and `insert` bind the text, then the
/// rowid; `insert` also binds the entity id.
pub(super) async fn fts_set(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    text: &str,
    update: &'static str,
    insert: &'static str,
) -> Result<(), AppError> {
    let rowid = fts_rowid(tx, entity_id).await?;
    // Runtime (non-macro) queries: sqlx's compile-time introspection chokes on
    // FTS5 virtual tables, so all `entity_fts` access is unchecked.
    let updated = sqlx::query(update)
        .bind(text)
        .bind(rowid)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    if updated == 0 {
        sqlx::query(insert)
            .bind(text)
            .bind(rowid)
            .bind(entity_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Sync the FTS `name` column for an entity, leaving `body` (owned by
/// `RichTextService.PutRichText`) as it is. Call inside the same tx as the
/// entities-table write.
pub(crate) async fn fts_upsert_name(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    name: &str,
) -> Result<(), AppError> {
    fts_set(
        tx,
        entity_id,
        name,
        "UPDATE entity_fts SET name = ? WHERE rowid = ?",
        "INSERT INTO entity_fts (name, rowid, entity_id, body) VALUES (?, ?, ?, '')",
    )
    .await
}

/// Sync the FTS `body` column for an entity, leaving `name` (owned by
/// `EntityService`) as it is. Call inside the same tx as the richtext write.
pub(crate) async fn fts_upsert_body(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    body: &str,
) -> Result<(), AppError> {
    fts_set(
        tx,
        entity_id,
        body,
        "UPDATE entity_fts SET body = ? WHERE rowid = ?",
        "INSERT INTO entity_fts (body, rowid, entity_id, name) VALUES (?, ?, ?, '')",
    )
    .await
}

/// Remove an entity's FTS row. Call inside the same tx as the entity delete,
/// before it: FTS5 rows don't cascade, and the delete cascades away the
/// `entity_fts_rowids` row that holds the rowid.
pub(crate) async fn fts_delete(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM entity_fts
         WHERE rowid = (SELECT fts_rowid FROM entity_fts_rowids WHERE entity_id = ?)",
    )
    .bind(entity_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
