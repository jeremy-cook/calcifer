use tonic::Status;

use super::lookup::structure_and_facets;
use super::validate::reject_reserved_property_id;
use super::{EntityService, RELATED_SCOPE};
use crate::db::begin_write;
use crate::error::AppError;
use crate::links::relations::sync_relation_property;
use crate::links::store::{replace_scoped_links, LinkTarget};
use crate::structures::{self, PropertyKind};
use calcifer::v1::{
    property_value, watch_entities_response, Entity, EntityRef, EntityRefList, PropertyValue,
};

impl EntityService {
    /// The shared body of AddRelationTarget and RemoveRelationTarget: change one
    /// target of a `relations` property in one transaction, keeping the links
    /// derived from the property in step. A no-op (target already there, or
    /// already absent) writes nothing and publishes nothing. Remove never checks
    /// the target's existence or structure, so a stale ref can always go.
    pub(super) async fn edit_relation_target(
        &self,
        op: RelationTargetOp,
        entity_id: &str,
        property_id: &str,
        target_id: &str,
    ) -> Result<Entity, Status> {
        for (field, value) in [
            ("entity_id", entity_id),
            ("property_id", property_id),
            ("target_id", target_id),
        ] {
            if value.is_empty() {
                return Err(Status::invalid_argument(format!("{field} is required")));
            }
        }
        reject_reserved_property_id(property_id)?;
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;
        let (structure_type, facets) = structure_and_facets(&mut tx, entity_id).await?;
        let facets: Vec<&str> = facets.iter().map(String::as_str).collect();

        if let Some(facet) = structures::facet_of_property(property_id) {
            if !facets.contains(&facet.id) {
                return Err(Status::failed_precondition(format!(
                    "attach the {} facet before editing {property_id}",
                    facet.id
                )));
            }
        }
        let def = structures::property_for(&structure_type, &facets, property_id)
            .filter(|def| def.kind == PropertyKind::Relations)
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "{structure_type}.{property_id} is not a relations property"
                ))
            })?;

        // The ref an Add appends, carrying the target's real type.
        let added = match op {
            RelationTargetOp::Add => {
                let target_type = sqlx::query_scalar!(
                    "SELECT structure_type FROM entities WHERE id = ?",
                    target_id
                )
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| Status::not_found(format!("entity {target_id}")))?;
                if let Some(expected) = def.target_structure {
                    if target_type != expected {
                        return Err(Status::invalid_argument(format!(
                            "{structure_type}.{property_id} must reference a {expected}; \
                             {target_id} is a {target_type}"
                        )));
                    }
                }
                Some(EntityRef {
                    id: target_id.to_string(),
                    structure_type: target_type,
                })
            }
            RelationTargetOp::Remove => None,
        };

        // A missing row, or a value that isn't a `relations` list, is an empty list.
        let stored = sqlx::query_scalar!(
            "SELECT value_blob FROM properties WHERE entity_id = ? AND property_id = ?",
            entity_id,
            property_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .map(|blob| <PropertyValue as prost::Message>::decode(&*blob))
        .transpose()
        .map_err(AppError::from)?;
        let mut refs = match stored.and_then(|v| v.value) {
            Some(property_value::Value::Relations(list)) => list.refs,
            _ => Vec::new(),
        };

        let present = refs.iter().any(|r| r.id == target_id);
        let changes = match op {
            RelationTargetOp::Add => !present,
            RelationTargetOp::Remove => present,
        };
        if !changes {
            drop(tx);
            return self.load_entity(entity_id).await.map_err(Status::from);
        }
        match added {
            Some(added) => refs.push(added),
            None => refs.retain(|r| r.id != target_id),
        }

        // Emptying the list clears the property, as SetEntityProperty's clear does.
        let value = (!refs.is_empty()).then_some(PropertyValue {
            value: Some(property_value::Value::Relations(EntityRefList { refs })),
        });
        match &value {
            Some(value) => {
                let blob = prost::Message::encode_to_vec(value);
                sqlx::query!(
                    "INSERT INTO properties (entity_id, property_id, value_blob) VALUES (?, ?, ?)
                     ON CONFLICT (entity_id, property_id) DO UPDATE SET value_blob = excluded.value_blob",
                    entity_id,
                    property_id,
                    blob,
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;
            }
            None => {
                sqlx::query!(
                    "DELETE FROM properties WHERE entity_id = ? AND property_id = ?",
                    entity_id,
                    property_id
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;
            }
        }
        sync_relation_property(
            &mut tx,
            entity_id,
            &structure_type,
            property_id,
            value.as_ref().and_then(|v| v.value.as_ref()),
            now,
        )
        .await
        .map_err(Status::from)?;
        sqlx::query!(
            "UPDATE entities SET updated_at = ? WHERE id = ?",
            now,
            entity_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(entity_id).await.map_err(Status::from)?;
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        Ok(saved)
    }

    /// The shared body of AddRelationship and RemoveRelationship (ADR 12): insert or
    /// delete one `relationships` row and re-derive the source's `@related` links
    /// from the table, in one transaction. A no-op (relationship already there, or
    /// already absent) writes nothing and publishes nothing.
    ///
    /// Add validates against the current registry before it looks at the row, so
    /// `relates_to` decides whether the source may relate to the target now. Remove
    /// checks only that the source exists, so a row left behind by a narrowed list
    /// can always go.
    pub(super) async fn edit_relationship(
        &self,
        op: RelationshipOp,
        entity_id: &str,
        target_id: &str,
    ) -> Result<Entity, Status> {
        for (field, value) in [("entity_id", entity_id), ("target_id", target_id)] {
            if value.is_empty() {
                return Err(Status::invalid_argument(format!("{field} is required")));
            }
        }
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;
        let structure_type = sqlx::query_scalar!(
            "SELECT structure_type FROM entities WHERE id = ?",
            entity_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| Status::not_found(format!("entity {entity_id}")))?;

        let changed = match op {
            RelationshipOp::Add => {
                let relates_to =
                    structures::structure(&structure_type).map_or(&[][..], |s| s.relates_to);
                if relates_to.is_empty() {
                    return Err(Status::failed_precondition(format!(
                        "a {structure_type} can't hold relationships"
                    )));
                }
                let target_type = sqlx::query_scalar!(
                    "SELECT structure_type FROM entities WHERE id = ?",
                    target_id
                )
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| Status::not_found(format!("entity {target_id}")))?;
                if target_id == entity_id {
                    return Err(Status::invalid_argument(
                        "an entity can't be related to itself",
                    ));
                }
                if !relates_to.contains(&target_type.as_str()) {
                    return Err(Status::invalid_argument(format!(
                        "a {structure_type} can't relate to a {target_type}"
                    )));
                }
                sqlx::query!(
                    "INSERT INTO relationships (entity_id, target_id, created_at) VALUES (?, ?, ?)
                     ON CONFLICT (entity_id, target_id) DO NOTHING",
                    entity_id,
                    target_id,
                    now,
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?
                .rows_affected()
                    > 0
            }
            RelationshipOp::Remove => {
                sqlx::query!(
                    "DELETE FROM relationships WHERE entity_id = ? AND target_id = ?",
                    entity_id,
                    target_id,
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?
                .rows_affected()
                    > 0
            }
        };
        if !changed {
            drop(tx);
            return self.load_entity(entity_id).await.map_err(Status::from);
        }

        // The table is the source: re-derive every `@related` link from it.
        let targets: Vec<LinkTarget> = sqlx::query!(
            r#"SELECT r.target_id AS "target_id!", e.structure_type AS "structure_type!"
               FROM relationships r JOIN entities e ON e.id = r.target_id
               WHERE r.entity_id = ? ORDER BY r.created_at, r.target_id"#,
            entity_id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::from)?
        .into_iter()
        .map(|row| LinkTarget {
            id: row.target_id,
            structure_type: row.structure_type,
        })
        .collect();
        replace_scoped_links(&mut tx, entity_id, RELATED_SCOPE, &targets, now)
            .await
            .map_err(Status::from)?;
        sqlx::query!(
            "UPDATE entities SET updated_at = ? WHERE id = ?",
            now,
            entity_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(entity_id).await.map_err(Status::from)?;
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        Ok(saved)
    }
}

/// Which way `EntityService::edit_relation_target` changes a `relations` property.
#[derive(Clone, Copy)]
pub(super) enum RelationTargetOp {
    Add,
    Remove,
}

/// Which way `EntityService::edit_relationship` changes a relationship.
#[derive(Clone, Copy)]
pub(super) enum RelationshipOp {
    Add,
    Remove,
}

/// Remove `deleted_id` from other entities' `relation` and `relations` values
/// (I-14): a `relations` value drops the matching refs and is deleted if none are
/// left; a `relation` value pointing at it is deleted. Each changed entity's
/// `updated_at` becomes `now`. Returns the changed entity ids, sorted.
///
/// Candidates come from the inbound `links` rows, so call it before they're swept.
/// A rich-text link has no property row (its document lives in `richtext`), and
/// any row that isn't a relation value is left alone.
pub(super) async fn strip_relation_refs(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    deleted_id: &str,
    now: i64,
) -> Result<Vec<String>, AppError> {
    let sources = sqlx::query!(
        r#"SELECT DISTINCT entity_id AS "entity_id!", source_property_id AS "property_id!"
           FROM links WHERE target_id = ? ORDER BY entity_id, source_property_id"#,
        deleted_id
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut changed: Vec<String> = Vec::new();
    for source in sources {
        let Some(blob) = sqlx::query_scalar!(
            "SELECT value_blob FROM properties WHERE entity_id = ? AND property_id = ?",
            source.entity_id,
            source.property_id
        )
        .fetch_optional(&mut **tx)
        .await?
        else {
            continue;
        };
        let value: PropertyValue = prost::Message::decode(&*blob)?;

        // Some(Some(v)): write v back; Some(None): delete the row; None: unchanged.
        let update = match value.value {
            Some(property_value::Value::Relation(r)) if r.id == deleted_id => Some(None),
            Some(property_value::Value::Relations(mut list)) => {
                let before = list.refs.len();
                list.refs.retain(|r| r.id != deleted_id);
                match list.refs.len() {
                    n if n == before => None,
                    0 => Some(None),
                    _ => Some(Some(PropertyValue {
                        value: Some(property_value::Value::Relations(list)),
                    })),
                }
            }
            _ => None,
        };
        let Some(update) = update else {
            continue;
        };

        match update {
            Some(value) => {
                let blob = prost::Message::encode_to_vec(&value);
                sqlx::query!(
                    "UPDATE properties SET value_blob = ? WHERE entity_id = ? AND property_id = ?",
                    blob,
                    source.entity_id,
                    source.property_id
                )
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query!(
                    "DELETE FROM properties WHERE entity_id = ? AND property_id = ?",
                    source.entity_id,
                    source.property_id
                )
                .execute(&mut **tx)
                .await?;
            }
        }
        if changed.last() != Some(&source.entity_id) {
            sqlx::query!(
                "UPDATE entities SET updated_at = ? WHERE id = ?",
                now,
                source.entity_id
            )
            .execute(&mut **tx)
            .await?;
            changed.push(source.entity_id);
        }
    }
    Ok(changed)
}
