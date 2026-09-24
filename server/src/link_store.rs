//! Link persistence: scoped writes to the `links` table. The pure extraction side
//! (what a doc references) lives in `links.rs`; this module owns how those
//! references, and relation-property values, become rows.

use crate::error::AppError;
use crate::links::MentionRef;
use crate::proto::{property_value, Entity};
use crate::structures::relation_properties;

/// Scoped link replace for a single (entity_id, source_property_id): select existing
/// link_id/created_at pairs for that scope, delete them, then re-insert one row per
/// target, reusing the prior link_id/created_at when a target repeats (preserves
/// backlink recency across an edit) and skipping targets with no `entities` row.
/// Runs inside an already-open transaction. Shared by `RichTextService.Put`
/// (content-derived links) and `sync_relation_links` (relation-property links).
pub(crate) async fn replace_scoped_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    source_property_id: &str,
    targets: &[MentionRef],
    now: i64,
) -> Result<(), AppError> {
    let existing = sqlx::query!(
        "SELECT link_id, target_id, created_at FROM links
         WHERE entity_id = ? AND source_property_id = ?",
        entity_id,
        source_property_id
    )
    .fetch_all(&mut **tx)
    .await?;
    let prev: std::collections::HashMap<String, (String, i64)> = existing
        .into_iter()
        .map(|row| (row.target_id, (row.link_id, row.created_at)))
        .collect();

    sqlx::query!(
        "DELETE FROM links WHERE entity_id = ? AND source_property_id = ?",
        entity_id,
        source_property_id
    )
    .execute(&mut **tx)
    .await?;

    for m in targets {
        // Drop targets whose entity row no longer exists. Safe because the server is
        // authoritative — unlike the FE, where such a chip is a tombstone, not a deletion.
        // Runtime (non-macro) query: same vec0-virtual-table compile-time
        // introspection hazard documented at fts_upsert_name forces unchecked here.
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM entities WHERE id = ?")
            .bind(&m.id)
            .fetch_optional(&mut **tx)
            .await?
            .is_some();
        if !exists {
            continue;
        }

        let (link_id, created_at) = match prev.get(&m.id) {
            Some((lid, ts)) => (lid.clone(), *ts),
            None => (uuid::Uuid::new_v4().to_string(), now),
        };
        sqlx::query!(
            "INSERT INTO links (entity_id, link_id, target_id, target_structure, source_property_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            entity_id,
            link_id,
            m.id,
            m.structure_type,
            source_property_id,
            created_at,
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Scoped-replace links derived from an entity's `relation`/`relations` property
/// values (as opposed to richtext-derived links, owned by `RichTextService.Put`).
///
/// Iterates the structure's *declared* relation properties as well as any present
/// on the entity, so a relation property that was removed or emptied still has its
/// old link rows cleared (otherwise the target keeps a stale backlink).
pub(crate) async fn sync_relation_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity: &Entity,
    now: i64,
) -> Result<(), AppError> {
    let mut property_ids: Vec<&str> = relation_properties(&entity.structure_type).to_vec();
    for prop in &entity.properties {
        let is_relation = matches!(
            prop.value.as_ref().and_then(|v| v.value.as_ref()),
            Some(property_value::Value::Relation(_) | property_value::Value::Relations(_))
        );
        if is_relation && !property_ids.contains(&prop.id.as_str()) {
            property_ids.push(&prop.id);
        }
    }

    for property_id in property_ids {
        let value = entity
            .properties
            .iter()
            .find(|p| p.id == property_id)
            .and_then(|p| p.value.as_ref())
            .and_then(|v| v.value.as_ref());
        let targets: Vec<MentionRef> = match value {
            Some(property_value::Value::Relation(r)) => {
                vec![MentionRef {
                    id: r.id.clone(),
                    structure_type: r.structure_type.clone(),
                }]
            }
            Some(property_value::Value::Relations(list)) => list
                .refs
                .iter()
                .map(|r| MentionRef {
                    id: r.id.clone(),
                    structure_type: r.structure_type.clone(),
                })
                .collect(),
            _ => Vec::new(),
        };
        replace_scoped_links(tx, &entity.id, property_id, &targets, now).await?;
    }
    Ok(())
}
