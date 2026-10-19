//! Links derived from `relation` / `relations` property values, as opposed to
//! rich-text links, which `RichTextService.PutRichText` derives from a doc.

use super::store::{replace_scoped_links, LinkTarget};
use crate::error::AppError;
use crate::proto::{property_value, Entity, EntityRef};
use crate::structures::{self, relation_properties};

/// Scoped-replace links derived from an entity's `relation`/`relations` property
/// values (as opposed to richtext-derived links, owned by `RichTextService.PutRichText`).
/// Each existing target is checked by `check_relation_targets` first, so a bad
/// ref fails the whole write.
///
/// Iterates the structure's and the attached facets' *declared* relation
/// properties as well as any present on the entity, so a relation property that
/// was removed or emptied still has its old link rows cleared (otherwise the
/// target keeps a stale backlink).
pub(crate) async fn sync_relation_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity: &Entity,
    now: i64,
) -> Result<(), AppError> {
    let mut property_ids: Vec<&str> = relation_properties(&entity.structure_type);
    for facet in entity.facets.iter().filter_map(|f| structures::facet(f)) {
        property_ids.extend(structures::facet_relation_properties(facet));
    }
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
/// property's declared `target_structure` (a structure property, or a facet
/// property by its namespace). Ad-hoc properties have no declared target to
/// check. Refs to ids with no `entities` row are dropped, not
/// rejected: a deleted target stays in other entities' stored values, and
/// rejecting it would block every later write of those relation values.
async fn check_relation_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    structure_type: &str,
    property_id: &str,
    refs: &[&EntityRef],
) -> Result<Vec<LinkTarget>, AppError> {
    let expected = structures::property(structure_type, property_id)
        .or_else(|| {
            let facet = structures::facet_of_property(property_id)?;
            facet.properties.iter().find(|p| p.id == property_id)
        })
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
