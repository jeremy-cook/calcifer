//! Link persistence: scoped writes to the `links` table. The pure extraction side
//! (what a doc references) lives in `links.rs`; this module owns how those
//! references, and relation-property values, become rows.

use crate::error::AppError;
use crate::links::MentionRef;
use crate::proto::{property_value, Entity, EntityRef};
use crate::structures::{self, relation_properties};

/// A link target that exists, with its real `structure_type` (I-51).
#[derive(Debug, Clone)]
pub(crate) struct LinkTarget {
    pub id: String,
    pub structure_type: String,
}

/// Look up each mention's real `structure_type`, dropping targets with no
/// `entities` row. Safe because the server is authoritative — unlike the FE, where
/// such a chip is a tombstone, not a deletion. Links record the target's real type,
/// never a type the caller claims (I-51).
pub(crate) async fn live_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    refs: &[MentionRef],
) -> Result<Vec<LinkTarget>, AppError> {
    let mut targets = Vec::with_capacity(refs.len());
    for m in refs {
        // Runtime (non-macro) query: same vec0-virtual-table compile-time
        // introspection hazard documented at fts_upsert_name forces unchecked here.
        let structure_type =
            sqlx::query_scalar::<_, String>("SELECT structure_type FROM entities WHERE id = ?")
                .bind(&m.id)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(structure_type) = structure_type {
            targets.push(LinkTarget {
                id: m.id.clone(),
                structure_type,
            });
        }
    }
    Ok(targets)
}

/// Scoped link replace for a single (entity_id, source_property_id): select existing
/// link_id/created_at pairs for that scope, delete them, then re-insert one row per
/// target, reusing the prior link_id/created_at when a target repeats (preserves
/// backlink recency across an edit). Targets come from `live_targets` or
/// `check_relation_targets`, so they exist and carry their real type.
/// Runs inside an already-open transaction. Shared by `RichTextService.PutRichText`
/// (content-derived links) and `sync_relation_property` (relation-property links).
pub(crate) async fn replace_scoped_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    source_property_id: &str,
    targets: &[LinkTarget],
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
/// values (as opposed to richtext-derived links, owned by `RichTextService.PutRichText`).
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
    // Ad-hoc relation properties in id order, so the map's order doesn't leak.
    let mut ad_hoc: Vec<&str> = entity
        .properties
        .iter()
        .filter(|(id, v)| {
            is_relation_value(v.value.as_ref()) && !property_ids.contains(&id.as_str())
        })
        .map(|(id, _)| id.as_str())
        .collect();
    ad_hoc.sort_unstable();
    property_ids.extend(ad_hoc);

    for property_id in property_ids {
        let value = entity
            .properties
            .get(property_id)
            .and_then(|v| v.value.as_ref());
        sync_relation_property(
            tx,
            &entity.id,
            &entity.structure_type,
            property_id,
            value,
            now,
        )
        .await?;
    }
    Ok(())
}

/// Whether a property value is a `relation`/`relations` value (and so owns link rows).
pub(crate) fn is_relation_value(value: Option<&property_value::Value>) -> bool {
    matches!(
        value,
        Some(property_value::Value::Relation(_) | property_value::Value::Relations(_))
    )
}

/// Scoped-replace the links of one relation property from its new `value`
/// (`None`, or a non-relation value, clears them). The per-property step of
/// `sync_relation_links`, also called on its own by `EntityService.SetEntityProperty`.
/// Callers decide which properties are relation properties; this doesn't check.
pub(crate) async fn sync_relation_property(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    structure_type: &str,
    property_id: &str,
    value: Option<&property_value::Value>,
    now: i64,
) -> Result<(), AppError> {
    let refs: Vec<&EntityRef> = match value {
        Some(property_value::Value::Relation(r)) => vec![r],
        Some(property_value::Value::Relations(list)) => list.refs.iter().collect(),
        _ => Vec::new(),
    };
    let targets = check_relation_targets(tx, structure_type, property_id, &refs).await?;
    replace_scoped_links(tx, entity_id, property_id, &targets, now).await
}

/// Check a relation property's refs against the targets' real `structure_type`
/// (I-2) and return them as link targets carrying that real type (one read per
/// ref), so the stored value and its links never disagree. Rejects a ref whose
/// claimed type (when non-empty) isn't the target's, or whose target isn't the
/// property's declared `target_structure`. Ad-hoc properties have no declared
/// target to check. Refs to ids with no `entities` row are dropped, not
/// rejected: a deleted target stays in other entities' stored values, and
/// rejecting it would block every later write of those relation values.
async fn check_relation_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    structure_type: &str,
    property_id: &str,
    refs: &[&EntityRef],
) -> Result<Vec<LinkTarget>, AppError> {
    let expected =
        structures::property(structure_type, property_id).and_then(|def| def.target_structure);
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
                "{structure_type}.{property_id}: relation target {} is a {actual}, not a {}",
                r.id, r.structure_type
            )));
        }
        if let Some(expected) = expected {
            if actual != expected {
                return Err(AppError::Invalid(format!(
                    "{structure_type}.{property_id} must reference a {expected}; {} is a {actual}",
                    r.id
                )));
            }
        }
        targets.push(LinkTarget {
            id: r.id.clone(),
            structure_type: actual,
        });
    }
    Ok(targets)
}
