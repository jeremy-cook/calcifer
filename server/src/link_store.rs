//! Link persistence: scoped writes to the `links` table. The pure extraction side
//! (what a doc references) lives in `links.rs`; this module owns how those
//! references, and relation-property values, become rows.

use crate::error::AppError;
use crate::links::MentionRef;
use crate::proto::{property_value, Entity, EntityRef};
use crate::structures::{self, relation_properties};

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
/// Each existing target is checked by `check_relation_targets` first, so a bad
/// ref fails the whole write.
///
/// Iterates the structure's *declared* relation properties as well as any present
/// on the entity, so a relation property that was removed or emptied still has its
/// old link rows cleared (otherwise the target keeps a stale backlink).
pub(crate) async fn sync_relation_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity: &Entity,
    now: i64,
) -> Result<(), AppError> {
    let mut property_ids: Vec<&str> = relation_properties(&entity.structure_type);
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
        let refs: Vec<&EntityRef> = match value {
            Some(property_value::Value::Relation(r)) => vec![r],
            Some(property_value::Value::Relations(list)) => list.refs.iter().collect(),
            _ => Vec::new(),
        };
        let targets = check_relation_targets(tx, entity, property_id, &refs).await?;
        replace_scoped_links(tx, &entity.id, property_id, &targets, now).await?;
    }
    Ok(())
}

/// Check a relation property's refs against the targets' real `structure_type`
/// (I-2) and return them as link targets carrying that real type, so the stored
/// value and its links never disagree. Rejects a ref whose claimed type (when
/// non-empty) isn't the target's, or whose target isn't the property's declared
/// `target_structure`. Ad-hoc properties have no declared target to check.
/// Refs to ids with no `entities` row are dropped, not rejected: a deleted
/// target stays in other entities' stored values, and rejecting it would block
/// every later Update of those entities.
async fn check_relation_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity: &Entity,
    property_id: &str,
    refs: &[&EntityRef],
) -> Result<Vec<MentionRef>, AppError> {
    let expected = structures::property(&entity.structure_type, property_id)
        .and_then(|def| def.target_structure);
    let mut targets = Vec::with_capacity(refs.len());
    for r in refs {
        let actual = sqlx::query_scalar!("SELECT structure_type FROM entities WHERE id = ?", r.id)
            .fetch_optional(&mut **tx)
            .await?;
        let Some(actual) = actual else {
            continue;
        };
        if !r.structure_type.is_empty() && r.structure_type != actual {
            return Err(AppError::Invalid(format!(
                "{}.{property_id}: relation target {} is a {actual}, not a {}",
                entity.structure_type, r.id, r.structure_type
            )));
        }
        if let Some(expected) = expected {
            if actual != expected {
                return Err(AppError::Invalid(format!(
                    "{}.{property_id} must reference a {expected}; {} is a {actual}",
                    entity.structure_type, r.id
                )));
            }
        }
        targets.push(MentionRef {
            id: r.id.clone(),
            structure_type: actual,
        });
    }
    Ok(targets)
}
