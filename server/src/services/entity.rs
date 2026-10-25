use std::collections::HashMap;

use futures::stream::BoxStream;
use sqlx::SqlitePool;
use tokio::sync::broadcast::{self, error::RecvError};
use tonic::{Request, Response, Status};

use crate::dates::{format_long_date, is_iso_day};
use crate::db::begin_write;
use crate::embed::EmbedHandle;
use crate::error::AppError;
use crate::links::relations::{is_relation_value, sync_relation_links, sync_relation_property};
use crate::links::store::{replace_scoped_links, LinkTarget};
use crate::proto::{
    entity_service_server::EntityService as EntityServiceTrait, property_value,
    resolve_entity_request, watch_entities_response, AddRelationTargetRequest,
    AddRelationTargetResponse, AddRelationshipRequest, AddRelationshipResponse, AttachFacetRequest,
    AttachFacetResponse, Backlink, CreateEntityRequest, CreateEntityResponse, DeleteEntityRequest,
    DeleteEntityResponse, DetachFacetRequest, DetachFacetResponse, Entity, EntityRef,
    EntityRefList, EntitySnapshot, GetEntityRequest, GetEntityResponse, LinkRef,
    ListBacklinksRequest, ListBacklinksResponse, ListEntitiesRequest, ListEntitiesResponse,
    PropertyValue, RemoveRelationTargetRequest, RemoveRelationTargetResponse,
    RemoveRelationshipRequest, RemoveRelationshipResponse, RenameEntityRequest,
    RenameEntityResponse, ResolveEntityRequest, ResolveEntityResponse, SetEntityPropertyRequest,
    SetEntityPropertyResponse, WatchEntitiesRequest, WatchEntitiesResponse,
};
use crate::structures::{self, fold_name, FacetDef, KeyDef, KeySource, PropertyKind};
use crate::watch::WatchHub;

/// The reserved link scope a relationship's links are derived under (ADR 12, rules 8
/// and 9). No property can use it: ids starting with `@` are refused.
const RELATED_SCOPE: &str = "@related";

pub struct EntityService {
    pool: SqlitePool,
    hub: WatchHub,
    embed: EmbedHandle,
}

impl EntityService {
    pub fn new(pool: SqlitePool, hub: WatchHub, embed: EmbedHandle) -> Self {
        Self { pool, hub, embed }
    }

    /// Insert a brand-new entity built by `new_entity` (row, properties, relation
    /// links and FTS name) in one tx and return the hydrated result. Shared by
    /// Create and Resolve. A new entity has no content, so no content-derived
    /// links or referenced dates: those are `RichTextService.PutRichText`'s (ADR 3).
    /// Every key the structure declares is written too, so a taken one is
    /// `AlreadyExists` (see `map_unique_violation`), and so is an
    /// `entity_facets` row per attached facet.
    async fn persist_new_entity(&self, entity: &Entity) -> Result<Entity, Status> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;
        sqlx::query!(
            "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
            entity.id,
            entity.structure_type,
            entity.name,
            now,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        for facet in &entity.facets {
            sqlx::query!(
                "INSERT INTO entity_facets (entity_id, facet) VALUES (?, ?)",
                entity.id,
                facet,
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }
        self.replace_properties(&mut tx, entity).await?;
        for def in declared_keys(&entity.structure_type) {
            let value = key_value(def.source, &entity.name, &entity.properties);
            let shown = shown_key_value(def.source, &entity.name, value.as_deref());
            set_key(
                &mut tx,
                &entity.id,
                &entity.structure_type,
                def.key,
                value.as_deref(),
            )
            .await
            .map_err(|e| map_unique_violation(e, &entity.structure_type, def, shown))?;
        }
        sync_relation_links(&mut tx, entity, now).await?;
        fts_upsert_name(&mut tx, &entity.id, &entity.name).await?;
        tx.commit().await.map_err(AppError::from)?;
        // New entity: queue for embedding (no-op until it has content, but keeps
        // the path uniform — Resolve's creates flow here too).
        self.embed.enqueue(&entity.id);
        Ok(self.load_entity(&entity.id).await?)
    }

    /// Case-insensitive lookup of an entity id by (structure_type, name). A
    /// structure with a name key (Tag) is looked up in `entity_keys`, folded as
    /// the write path folds it, so the lookup and the uniqueness rule agree.
    /// Other names aren't unique, so several entities can match: the oldest
    /// `created_at` wins and `id` breaks ties, so a lookup always returns the
    /// same one.
    async fn find_by_name(
        &self,
        structure_type: &str,
        name: &str,
    ) -> Result<Option<String>, AppError> {
        if let Some(def) = structures::structure(structure_type).and_then(|s| s.name_key()) {
            return self
                .find_by_key(structure_type, def.key, &fold_name(name))
                .await;
        }
        let row = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM entities
               WHERE structure_type = ? AND name = ? COLLATE NOCASE
               ORDER BY created_at, id
               LIMIT 1"#,
            structure_type,
            name
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// `base`, or if a `structure_type` entity already has that name
    /// (case-insensitively), the first free `"<base> N"` from N = 2. For the
    /// default name of a `unique_names` structure, so a second "+ New Tag"
    /// doesn't clash with the first.
    async fn free_name(&self, structure_type: &str, base: &str) -> Result<String, AppError> {
        let mut candidate = base.to_string();
        let mut n = 1;
        while self
            .find_by_name(structure_type, &candidate)
            .await?
            .is_some()
        {
            n += 1;
            candidate = format!("{base} {n}");
        }
        Ok(candidate)
    }

    /// The `structure_type` entity whose `key` has the canonical `value`.
    async fn find_by_key(
        &self,
        structure_type: &str,
        key: &str,
        value: &str,
    ) -> Result<Option<String>, AppError> {
        let row = sqlx::query_scalar!(
            "SELECT entity_id FROM entity_keys
             WHERE structure_type = ? AND key = ? AND value = ?",
            structure_type,
            key,
            value
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Resolve's `name` key: get the (structure_type, name) entity, or create it
    /// with its structure's defaults.
    async fn resolve_name(
        &self,
        structure_type: &str,
        name: &str,
        create_if_missing: bool,
    ) -> Result<ResolveEntityResponse, Status> {
        let name = name.trim();
        if structure_type.is_empty() {
            return Err(Status::invalid_argument(
                "structure_type is required to resolve by name",
            ));
        }
        let def = known_structure(structure_type)?;
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }

        // Get: a case-insensitive name match is the canonical entity.
        if let Some(id) = self
            .find_by_name(structure_type, name)
            .await
            .map_err(Status::from)?
        {
            let entity = self.load_entity(&id).await.map_err(Status::from)?;
            return Ok(ResolveEntityResponse {
                entity: Some(entity),
                created: false,
            });
        }
        if !create_if_missing {
            return Err(Status::not_found(format!(
                "{structure_type} named {name:?}"
            )));
        }
        // As CreateEntity refuses them (I-46): a DailyNote is created by its date.
        if !def.creatable {
            let hint = if structure_type == "DailyNote" {
                "; use ResolveEntity by date to get or create one"
            } else {
                ""
            };
            return Err(Status::failed_precondition(format!(
                "{structure_type} can't be created by name{hint}"
            )));
        }

        let entity = new_entity(structure_type, Some(name), HashMap::new(), &[]);
        let saved = self.persist_new_entity(&entity).await?;
        Ok(self.created(saved))
    }

    /// Resolve's `date` key: get the day's DailyNote, or create it (named for the
    /// day by `new_entity`).
    async fn resolve_date(
        &self,
        structure_type: &str,
        date: &str,
        create_if_missing: bool,
    ) -> Result<ResolveEntityResponse, Status> {
        if !structure_type.is_empty() && structure_type != "DailyNote" {
            return Err(Status::invalid_argument(format!(
                "resolving by date finds a DailyNote; structure_type must be empty or DailyNote, got {structure_type:?}"
            )));
        }
        // Canonical form only, so one day can't get a second key spelling.
        let date = date.trim();
        if !is_iso_day(date) {
            return Err(Status::invalid_argument(format!(
                "date must be an ISO calendar day (yyyy-MM-dd), got {date:?}"
            )));
        }

        if let Some(id) = self
            .find_by_key("DailyNote", "date", date)
            .await
            .map_err(Status::from)?
        {
            let entity = self.load_entity(&id).await.map_err(Status::from)?;
            return Ok(ResolveEntityResponse {
                entity: Some(entity),
                created: false,
            });
        }
        if !create_if_missing {
            return Err(Status::not_found(format!("DailyNote for {date}")));
        }

        let entity = new_entity("DailyNote", None, date_properties(date), &[]);
        let saved = self.persist_new_entity(&entity).await?;
        Ok(self.created(saved))
    }

    /// Publish a Resolve-created entity and wrap it in the response.
    fn created(&self, saved: Entity) -> ResolveEntityResponse {
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        ResolveEntityResponse {
            entity: Some(saved),
            created: true,
        }
    }

    /// Hydrate a full Entity (metadata + properties + links + referenced_dates).
    /// Shared by every RPC that returns an entity, so the read shape is defined once.
    async fn load_entity(&self, id: &str) -> Result<Entity, AppError> {
        load_entity(&self.pool, id).await
    }

    /// Replace an entity's properties wholesale inside a transaction, after
    /// checking them against the structure (see `validate_properties`).
    async fn replace_properties(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        entity: &Entity,
    ) -> Result<(), AppError> {
        validate_properties(entity)?;
        sqlx::query!("DELETE FROM properties WHERE entity_id = ?", entity.id)
            .execute(&mut **tx)
            .await?;

        for (property_id, value) in &entity.properties {
            // A map entry can't omit its value, so an empty one stands for "missing".
            if value.value.is_none() {
                return Err(AppError::Invalid("property missing value".to_string()));
            }
            let blob = prost::Message::encode_to_vec(value);
            sqlx::query!(
                "INSERT INTO properties (entity_id, property_id, value_blob) VALUES (?, ?, ?)",
                entity.id,
                property_id,
                blob,
            )
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// The shared body of AddRelationTarget and RemoveRelationTarget: change one
    /// target of a `relations` property in one transaction, keeping the links
    /// derived from the property in step. A no-op (target already there, or
    /// already absent) writes nothing and publishes nothing. Remove never checks
    /// the target's existence or structure, so a stale ref can always go.
    async fn edit_relation_target(
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
    async fn edit_relationship(
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
enum RelationTargetOp {
    Add,
    Remove,
}

/// Which way `EntityService::edit_relationship` changes a relationship.
#[derive(Clone, Copy)]
enum RelationshipOp {
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
async fn strip_relation_refs(
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

/// Hydrate a full Entity (metadata + properties + links + referenced_dates) from
/// the pool. The single read-shape definition, shared by EntityService and
/// SearchService so both services return identical entities.
pub(crate) async fn load_entity(pool: &SqlitePool, id: &str) -> Result<Entity, AppError> {
    let row = sqlx::query!(
        r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                  created_at AS "created_at!", updated_at AS "updated_at!"
           FROM entities WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("entity {}", id)))?;

    let prop_rows = sqlx::query!(
        r#"SELECT property_id AS "property_id!", value_blob AS "value_blob!"
           FROM properties WHERE entity_id = ?"#,
        id
    )
    .fetch_all(pool)
    .await?;

    let properties = prop_rows
        .into_iter()
        .map(|r| {
            let value: PropertyValue = prost::Message::decode(&*r.value_blob)?;
            Ok::<_, AppError>((r.property_id, value))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    let link_rows = sqlx::query!(
        r#"SELECT link_id AS "link_id!", target_id AS "target_id!",
                  target_structure AS "target_structure!",
                  source_property_id AS "source_property_id!", created_at AS "created_at!"
           FROM links WHERE entity_id = ? ORDER BY link_id"#,
        id
    )
    .fetch_all(pool)
    .await?;

    let links = link_rows
        .into_iter()
        .map(|r| LinkRef {
            id: r.link_id,
            target: Some(EntityRef {
                id: r.target_id,
                structure_type: r.target_structure,
            }),
            source_property_id: r.source_property_id,
            created_at: Some(ts_from_millis(r.created_at)),
        })
        .collect();

    let referenced_dates = sqlx::query_scalar!(
        r#"SELECT iso_date AS "iso_date!" FROM referenced_dates
           WHERE entity_id = ? ORDER BY iso_date"#,
        id
    )
    .fetch_all(pool)
    .await?;

    let mut facets = sqlx::query_scalar!(
        r#"SELECT facet AS "facet!" FROM entity_facets WHERE entity_id = ? ORDER BY facet"#,
        id
    )
    .fetch_all(pool)
    .await?;
    sort_facets(&mut facets);

    Ok(Entity {
        id: row.id,
        structure_type: row.structure_type,
        name: row.name,
        properties,
        links,
        referenced_dates,
        created_at: Some(ts_from_millis(row.created_at)),
        updated_at: Some(ts_from_millis(row.updated_at)),
        facets,
    })
}

/// Put facet ids in registry order (`structures::FACETS`). An id missing from
/// the registry is kept, after the known ones, in id order.
fn sort_facets(facets: &mut [String]) {
    facets.sort_by_cached_key(|f| {
        let rank = structures::FACETS
            .iter()
            .position(|d| d.id == f)
            .unwrap_or(usize::MAX);
        (rank, f.clone())
    });
}

/// An `entities` row, typed so both filter branches of `load_entities` share it.
struct EntityRow {
    id: String,
    structure_type: String,
    name: String,
    created_at: i64,
    updated_at: i64,
}

/// Hydrate every entity, or every entity of `structure_type`, most recently
/// updated first (ties by id), in five queries however many there are. Returns
/// what `load_entity` returns for each, field for field: both order links by
/// `link_id` and referenced dates by `iso_date` explicitly (I-48).
pub(crate) async fn load_entities(
    pool: &SqlitePool,
    structure_type: Option<&str>,
) -> Result<Vec<Entity>, AppError> {
    load_entities_where(pool, structure_type, None).await
}

/// Hydrate the entities whose ids are in `ids`, as `load_entities` does, in five
/// queries. Ids with no entity are left out; the result is most recently updated
/// first, not in `ids` order.
pub(crate) async fn load_entities_by_id(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<Entity>, AppError> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let ids = serde_json::to_string(ids).map_err(|e| AppError::Invalid(e.to_string()))?;
    load_entities_where(pool, None, Some(&ids)).await
}

/// The body of `load_entities` and `load_entities_by_id`. `ids_json`, when set,
/// is a JSON array of entity ids to restrict to (read with `json_each`).
async fn load_entities_where(
    pool: &SqlitePool,
    structure_type: Option<&str>,
    ids_json: Option<&str>,
) -> Result<Vec<Entity>, AppError> {
    // Literal queries per filter rather than `? IS NULL OR …`, so each List query
    // gets a plan for its own filter. The order is explicit either way: most
    // recently updated first, ties broken by id (I-48).
    let rows = match (structure_type, ids_json) {
        (None, None) => {
            sqlx::query_as!(
                EntityRow,
                r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                          created_at AS "created_at!", updated_at AS "updated_at!"
                   FROM entities ORDER BY updated_at DESC, id"#
            )
            .fetch_all(pool)
            .await?
        }
        (Some(structure_type), None) => {
            sqlx::query_as!(
                EntityRow,
                r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                          created_at AS "created_at!", updated_at AS "updated_at!"
                   FROM entities WHERE structure_type = ? ORDER BY updated_at DESC, id"#,
                structure_type
            )
            .fetch_all(pool)
            .await?
        }
        (_, Some(ids_json)) => {
            sqlx::query_as!(
                EntityRow,
                r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                          created_at AS "created_at!", updated_at AS "updated_at!"
                   FROM entities
                   WHERE id IN (SELECT value FROM json_each(?1))
                     AND (?2 IS NULL OR structure_type = ?2)
                   ORDER BY updated_at DESC, id"#,
                ids_json,
                structure_type
            )
            .fetch_all(pool)
            .await?
        }
    };

    let mut entities: Vec<Entity> = rows
        .into_iter()
        .map(|row| Entity {
            id: row.id,
            structure_type: row.structure_type,
            name: row.name,
            properties: HashMap::new(),
            links: vec![],
            referenced_dates: vec![],
            created_at: Some(ts_from_millis(row.created_at)),
            updated_at: Some(ts_from_millis(row.updated_at)),
            facets: vec![],
        })
        .collect();
    let index: HashMap<String, usize> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i))
        .collect();

    // Each child query is restricted by the same filter. A row whose entity isn't
    // in `index` (created between the queries) is skipped.
    let prop_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", property_id AS "property_id!",
                  value_blob AS "value_blob!"
           FROM properties
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE (?1 IS NULL OR structure_type = ?1)
                                 AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2))))
           ORDER BY entity_id, property_id"#,
        structure_type,
        ids_json
    )
    .fetch_all(pool)
    .await?;
    for r in prop_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        let value: PropertyValue = prost::Message::decode(&*r.value_blob)?;
        entities[i].properties.insert(r.property_id, value);
    }

    let link_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", link_id AS "link_id!", target_id AS "target_id!",
                  target_structure AS "target_structure!",
                  source_property_id AS "source_property_id!", created_at AS "created_at!"
           FROM links
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE (?1 IS NULL OR structure_type = ?1)
                                 AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2))))
           ORDER BY entity_id, link_id"#,
        structure_type,
        ids_json
    )
    .fetch_all(pool)
    .await?;
    for r in link_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        entities[i].links.push(LinkRef {
            id: r.link_id,
            target: Some(EntityRef {
                id: r.target_id,
                structure_type: r.target_structure,
            }),
            source_property_id: r.source_property_id,
            created_at: Some(ts_from_millis(r.created_at)),
        });
    }

    let date_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", iso_date AS "iso_date!"
           FROM referenced_dates
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE (?1 IS NULL OR structure_type = ?1)
                                 AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2))))
           ORDER BY entity_id, iso_date"#,
        structure_type,
        ids_json
    )
    .fetch_all(pool)
    .await?;
    for r in date_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        entities[i].referenced_dates.push(r.iso_date);
    }

    let facet_rows = sqlx::query!(
        r#"SELECT entity_id AS "entity_id!", facet AS "facet!"
           FROM entity_facets
           WHERE entity_id IN (SELECT id FROM entities
                               WHERE (?1 IS NULL OR structure_type = ?1)
                                 AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2))))
           ORDER BY entity_id, facet"#,
        structure_type,
        ids_json
    )
    .fetch_all(pool)
    .await?;
    for r in facet_rows {
        let Some(&i) = index.get(&r.entity_id) else {
            continue;
        };
        entities[i].facets.push(r.facet);
    }
    for entity in &mut entities {
        sort_facets(&mut entity.facets);
    }

    Ok(entities)
}

/// The entity's `entity_fts` rowid, from `entity_fts_rowids`, assigning one if
/// it has none yet. FTS5 can't look up its UNINDEXED `entity_id` column, so
/// every `entity_fts` read and write goes through this rowid.
async fn fts_rowid(
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
async fn fts_set(
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

/// Convert unix-millis to a protobuf Timestamp.
pub(crate) fn ts_from_millis(millis: i64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: millis / 1000,
        nanos: ((millis % 1000) * 1_000_000) as i32,
    }
}

/// The natural keys a structure declares (ADR 10). Structures missing from the
/// registry have none.
fn declared_keys(structure_type: &str) -> &'static [KeyDef] {
    structures::structure(structure_type).map_or(&[], |s| s.keys)
}

/// The canonical value of one natural key, from the entity's `name` and
/// `properties`. None when the source is empty, e.g. a DailyNote with no date.
fn key_value(
    source: KeySource,
    name: &str,
    properties: &HashMap<String, PropertyValue>,
) -> Option<String> {
    match source {
        KeySource::Name => Some(fold_name(name)).filter(|n| !n.is_empty()),
        KeySource::Property(id) => {
            property_key_value(properties.get(id).and_then(|v| v.value.as_ref()))
        }
    }
}

/// The key value a single property contributes, for SetProperty, which only sees
/// the one property. A key's property is canonical once `validate_property` has
/// passed it (a date is an ISO day), so it's used as is.
fn property_key_value(value: Option<&property_value::Value>) -> Option<String> {
    match value {
        Some(
            property_value::Value::Date(v)
            | property_value::Value::Text(v)
            | property_value::Value::Select(v),
        ) if !v.is_empty() => Some(v.clone()),
        _ => None,
    }
}

/// How a key value is shown in a conflict message: a name as the user typed it,
/// not folded; any other value as stored.
fn shown_key_value<'a>(source: KeySource, name: &'a str, value: Option<&'a str>) -> &'a str {
    match source {
        KeySource::Name => name,
        KeySource::Property(_) => value.unwrap_or_default(),
    }
}

/// Replace an entity's value for one key: delete its `(entity_id, key)` row, then
/// insert `value` if there is one. A value another entity of the structure holds
/// fails the `entity_keys` primary key (see `map_unique_violation`). Call inside
/// the same tx as the write that changed the key's source.
async fn set_key(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    structure_type: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM entity_keys WHERE entity_id = ? AND key = ?",
        entity_id,
        key
    )
    .execute(&mut **tx)
    .await?;
    if let Some(value) = value {
        sqlx::query!(
            "INSERT INTO entity_keys (structure_type, key, value, entity_id) VALUES (?, ?, ?, ?)",
            structure_type,
            key,
            value,
            entity_id,
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Check every property of an entity against its structure. See
/// `validate_property`.
fn validate_properties(entity: &Entity) -> Result<(), AppError> {
    let facets: Vec<&str> = entity.facets.iter().map(String::as_str).collect();
    for (property_id, value) in &entity.properties {
        validate_property(
            &entity.structure_type,
            &facets,
            property_id,
            value.value.as_ref(),
        )?;
    }
    Ok(())
}

/// Reject a property value that doesn't fit the structure's declaration: a
/// declared rich-text property takes no value at all, since its document is
/// addressed by (entity id, property id) through `RichTextService` (I-22); any
/// other declared property must hold a value of its declared `PropertyKind`
/// (I-16), a declared select one of its options, and a `select` value is only
/// allowed on a declared select (I-9).
/// Other undeclared (ad-hoc) properties take any value. Structures missing from
/// the registry (rows written before unknown types were rejected, I-24) have no
/// schema and pass unchecked. Any `date` value, declared or not and whatever
/// the structure, must be an ISO day (I-52). Shared by Create and Resolve (via
/// `validate_properties`) and SetProperty.
///
/// `facets` are the entity's attached facets (ADR 11): a facet property is
/// declared only while its facet is attached, and a value on an id in a
/// registered facet's namespace (`todo.status`) whose facet isn't attached is
/// `FailedPrecondition` (D6). Callers don't validate a clear, so clearing one
/// stays allowed.
fn validate_property(
    structure_type: &str,
    facets: &[&str],
    property_id: &str,
    value: Option<&property_value::Value>,
) -> Result<(), AppError> {
    if let Some(property_value::Value::Date(d)) = value {
        if !is_iso_day(d) {
            return Err(AppError::Invalid(format!(
                "{structure_type}.{property_id} must be an ISO calendar day (yyyy-MM-dd), got {d:?}"
            )));
        }
    }
    if let Some(facet) = structures::facet_of_property(property_id) {
        if value.is_some() && !facets.contains(&facet.id) {
            return Err(AppError::FailedPrecondition(format!(
                "attach the {} facet before setting {property_id}",
                facet.id
            )));
        }
    }
    if structures::structure(structure_type).is_none() {
        return Ok(());
    }
    let Some(def) = structures::property_for(structure_type, facets, property_id) else {
        return match value {
            Some(property_value::Value::Select(key)) => Err(AppError::Invalid(format!(
                "{structure_type}.{property_id} is not a select property (got select {key:?})"
            ))),
            _ => Ok(()),
        };
    };
    // Exhaustive, so a new kind must decide how it's checked.
    let expected = match def.kind {
        PropertyKind::Richtext => {
            return Err(AppError::Invalid(format!(
                "{structure_type}.{property_id} is a rich-text property and takes no value; \
                 its document is read and written with RichTextService"
            )));
        }
        PropertyKind::Select => return validate_select(structure_type, def, value),
        PropertyKind::Text => "text",
        PropertyKind::Number => "number",
        PropertyKind::Date => "date",
        PropertyKind::Relation => "relation",
        PropertyKind::Relations => "relations",
    };
    let got = value.map(case_name);
    if got != Some(expected) {
        return Err(AppError::Invalid(format!(
            "{structure_type}.{property_id} must be a {expected} value, got {}",
            got.unwrap_or("no value")
        )));
    }
    Ok(())
}

/// A declared select's value must be a `select` naming one of its options (I-9).
fn validate_select(
    structure_type: &str,
    def: &structures::PropertyDef,
    value: Option<&property_value::Value>,
) -> Result<(), AppError> {
    let allowed: Vec<&str> = def.options.iter().map(|o| o.key).collect();
    let Some(property_value::Value::Select(key)) = value else {
        return Err(AppError::Invalid(format!(
            "{structure_type}.{} must be a select value, one of: {}",
            def.id,
            allowed.join(", ")
        )));
    };
    if !allowed.contains(&key.as_str()) {
        return Err(AppError::Invalid(format!(
            "invalid value {key:?} for {structure_type}.{}; allowed: {}",
            def.id,
            allowed.join(", ")
        )));
    }
    Ok(())
}

/// A value's case as it is spelled in the proto, which is also the name of the
/// `PropertyKind` that takes it.
fn case_name(value: &property_value::Value) -> &'static str {
    match value {
        property_value::Value::Text(_) => "text",
        property_value::Value::Number(_) => "number",
        property_value::Value::Date(_) => "date",
        property_value::Value::Select(_) => "select",
        property_value::Value::Relation(_) => "relation",
        property_value::Value::Relations(_) => "relations",
    }
}

/// Map a `set_key` failure: a value another entity of the structure already
/// holds (the `entity_keys` primary key) is `AlreadyExists` with a message for
/// the key (I-25); anything else, including a clash on `(entity_id, key)`, which
/// would be a bug in `set_key`'s delete-then-insert, takes the standard AppError
/// -> Status conversion. `shown` is the value as the user gave it.
fn map_unique_violation(err: AppError, structure_type: &str, key: &KeyDef, shown: &str) -> Status {
    let AppError::Db(sqlx::Error::Database(ref db)) = err else {
        return Status::from(err);
    };
    // SQLite names a failed unique constraint by its column list. The tests
    // below pin the messages.
    if !db.is_unique_violation()
        || !db
            .message()
            .ends_with("entity_keys.structure_type, entity_keys.key, entity_keys.value")
    {
        return Status::from(err);
    }
    let message = match (structure_type, key.key, key.source) {
        ("DailyNote", "date", _) => format!("a DailyNote for {shown} already exists"),
        (_, _, KeySource::Name) => format!("a {structure_type} named \"{shown}\" already exists"),
        (_, key, _) => format!("a {structure_type} with {key} {shown:?} already exists"),
    };
    Status::already_exists(message)
}

/// The DailyNote name rule (ADR 8): a DailyNote is named for its date, as a long
/// human date, on every write. `date` is the value of a DailyNote's `date`
/// property, so this is None for an undated DailyNote, which keeps the name it
/// has.
fn daily_note_name(date: Option<&str>) -> Option<String> {
    date.map(format_long_date)
}

/// Apply `daily_note_name` to a whole entity, for `new_entity` (and so Create and
/// Resolve). SetProperty applies it to the one row.
fn apply_daily_note_name(entity: &mut Entity) {
    if entity.structure_type != "DailyNote" {
        return;
    }
    let date = property_key_value(entity.properties.get("date").and_then(|v| v.value.as_ref()));
    if let Some(name) = daily_note_name(date.as_deref()) {
        entity.name = name;
    }
}

/// Build a new, unsaved entity with every server-owned default (ADR 8): a minted
/// id, the `facets` (ADR 11, in registry order), and each select's default
/// option, the structure's and the facets', with the caller's `properties` laid
/// over them (the caller wins on the same id). Rich-text properties get no value: their
/// documents are addressed by declared property (I-22), and `validate_property`
/// rejects a caller value for one on persist. A DailyNote with a date is named for it
/// (`apply_daily_note_name`); otherwise the name is `name`, or with none the
/// structure's `default_name`. The caller makes a `unique_names` default free
/// (`EntityService::free_name`); this has no database.
fn new_entity(
    structure_type: &str,
    name: Option<&str>,
    properties: HashMap<String, PropertyValue>,
    facets: &[&FacetDef],
) -> Entity {
    let id = uuid::Uuid::new_v4().to_string();
    let mut select_defaults = structures::select_defaults(structure_type);
    for facet in facets {
        select_defaults.extend(structures::facet_select_defaults(facet));
    }
    let mut facets: Vec<String> = facets.iter().map(|f| f.id.to_string()).collect();
    sort_facets(&mut facets);
    let mut defaults: HashMap<String, PropertyValue> = select_defaults
        .iter()
        .map(|(pid, default)| {
            let value = PropertyValue {
                value: Some(property_value::Value::Select(default.to_string())),
            };
            (pid.to_string(), value)
        })
        .collect();
    defaults.extend(properties);
    let mut entity = Entity {
        id,
        structure_type: structure_type.to_string(),
        name: name.map_or_else(|| default_name(structure_type), str::to_string),
        properties: defaults,
        links: vec![],
        referenced_dates: vec![],
        created_at: None,
        updated_at: None,
        facets,
    };
    apply_daily_note_name(&mut entity);
    entity
}

/// The name of a new entity created without one: `Untitled <StructureDef.name>`,
/// e.g. "Untitled Note". Structures missing from the registry use their type.
fn default_name(structure_type: &str) -> String {
    let name = structures::structure(structure_type).map_or(structure_type, |s| s.name);
    format!("Untitled {name}")
}

/// A DailyNote's properties: just its `date`, holding the ISO day `date`.
fn date_properties(date: &str) -> HashMap<String, PropertyValue> {
    let value = PropertyValue {
        value: Some(property_value::Value::Date(date.to_string())),
    };
    HashMap::from([("date".to_string(), value)])
}

/// `InvalidArgument` for a property id that starts with `@`. Those ids are
/// reserved for the link scopes the server derives, such as `@related` (ADR 12,
/// rule 9), so no write may name one as a property.
pub(crate) fn reject_reserved_property_id(id: &str) -> Result<(), Status> {
    if id.starts_with('@') {
        return Err(Status::invalid_argument(
            "property ids starting with @ are reserved",
        ));
    }
    Ok(())
}

/// The registry entry for `structure_type`, or `InvalidArgument` for a type the
/// registry doesn't declare (I-24). Every write or lookup that takes a
/// `structure_type` goes through this, so an unknown type can't create an entity.
fn known_structure(structure_type: &str) -> Result<&'static structures::StructureDef, Status> {
    structures::structure(structure_type).ok_or_else(|| {
        Status::invalid_argument(format!("unknown structure_type {structure_type:?}"))
    })
}

/// The registry entry for facet `id`, or `InvalidArgument` for one the registry
/// doesn't declare.
fn known_facet(id: &str) -> Result<&'static FacetDef, Status> {
    structures::facet(id).ok_or_else(|| Status::invalid_argument(format!("unknown facet {id:?}")))
}

/// `known_facet`, and `FailedPrecondition` unless `structure_type` accepts it
/// (`StructureDef.facets`). Create and AttachFacet check a facet with this.
fn accepted_facet(structure_type: &str, id: &str) -> Result<&'static FacetDef, Status> {
    let facet = known_facet(id)?;
    let accepts =
        structures::structure(structure_type).is_some_and(|s| s.facets.contains(&facet.id));
    if !accepts {
        return Err(Status::failed_precondition(format!(
            "a {structure_type} doesn't accept the {} facet",
            facet.id
        )));
    }
    Ok(facet)
}

/// The facets attached to `entity_id`, read inside a write transaction.
async fn attached_facets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
) -> Result<Vec<String>, AppError> {
    let facets = sqlx::query_scalar!(
        r#"SELECT facet AS "facet!" FROM entity_facets WHERE entity_id = ?"#,
        entity_id
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(facets)
}

/// An entity's `structure_type` and attached facets, read inside a write
/// transaction, or `NotFound`.
async fn structure_and_facets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
) -> Result<(String, Vec<String>), Status> {
    let structure_type = sqlx::query_scalar!(
        "SELECT structure_type FROM entities WHERE id = ?",
        entity_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| Status::not_found(format!("entity {entity_id}")))?;
    let facets = attached_facets(tx, entity_id).await?;
    Ok((structure_type, facets))
}

#[tonic::async_trait]
impl EntityServiceTrait for EntityService {
    async fn get_entity(
        &self,
        req: Request<GetEntityRequest>,
    ) -> Result<Response<GetEntityResponse>, Status> {
        let id = req.into_inner().id;
        let entity = self.load_entity(&id).await.map_err(Status::from)?;
        Ok(Response::new(GetEntityResponse {
            entity: Some(entity),
        }))
    }

    async fn list_entities(
        &self,
        req: Request<ListEntitiesRequest>,
    ) -> Result<Response<ListEntitiesResponse>, Status> {
        let filter = req.into_inner().structure_type;
        // "" lists every structure.
        if !filter.is_empty() {
            known_structure(&filter)?;
        }

        let structure_type = (!filter.is_empty()).then_some(filter.as_str());
        let entities = load_entities(&self.pool, structure_type)
            .await
            .map_err(Status::from)?;

        Ok(Response::new(ListEntitiesResponse { entities }))
    }

    // Create by intent (ADR 8): the server builds the entity with `new_entity`,
    // so a client can't supply its id, links, referenced dates or timestamps.
    // Only `creatable` structures: a DailyNote comes from Resolve's date key.
    async fn create_entity(
        &self,
        req: Request<CreateEntityRequest>,
    ) -> Result<Response<CreateEntityResponse>, Status> {
        let CreateEntityRequest {
            structure_type,
            name,
            properties,
            facets,
        } = req.into_inner();
        for property_id in properties.keys() {
            reject_reserved_property_id(property_id)?;
        }
        let def = known_structure(&structure_type)?;
        if !def.creatable {
            return Err(Status::failed_precondition(format!(
                "{structure_type} can't be created with CreateEntity; use ResolveEntity to get or create one"
            )));
        }
        let mut facet_defs: Vec<&FacetDef> = Vec::with_capacity(facets.len());
        for id in &facets {
            let facet = accepted_facet(&structure_type, id)?;
            if !facet_defs.iter().any(|f| f.id == facet.id) {
                facet_defs.push(facet);
            }
        }
        let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let mut entity = new_entity(&structure_type, name.as_deref(), properties, &facet_defs);
        if name.is_none() && def.unique_names() {
            entity.name = self
                .free_name(&structure_type, &entity.name)
                .await
                .map_err(Status::from)?;
        }

        let saved = self.persist_new_entity(&entity).await?;
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        Ok(Response::new(CreateEntityResponse {
            entity: Some(saved),
        }))
    }

    // The name (and its FTS row) and updated_at only: properties are untouched,
    // so a rename doesn't undo a concurrent SetProperty (I-15). A structure with
    // `name_editable = false` (a DailyNote, named for its date) can't be renamed.
    // The name is trimmed, as Create trims it, and can't be blank.
    async fn rename_entity(
        &self,
        req: Request<RenameEntityRequest>,
    ) -> Result<Response<RenameEntityResponse>, Status> {
        let RenameEntityRequest { id, name } = req.into_inner();
        let name = name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name can't be blank"));
        }
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;
        let structure_type =
            sqlx::query_scalar!("SELECT structure_type FROM entities WHERE id = ?", id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| Status::not_found(format!("entity {id}")))?;
        if structures::structure(&structure_type).is_some_and(|s| !s.name_editable) {
            return Err(Status::failed_precondition(format!(
                "a {structure_type}'s name isn't editable"
            )));
        }
        sqlx::query!(
            "UPDATE entities SET name = ?, updated_at = ? WHERE id = ?",
            name,
            now,
            id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        for def in declared_keys(&structure_type)
            .iter()
            .filter(|k| k.source == KeySource::Name)
        {
            let value = key_value(def.source, name, &HashMap::new());
            set_key(&mut tx, &id, &structure_type, def.key, value.as_deref())
                .await
                .map_err(|e| map_unique_violation(e, &structure_type, def, name))?;
        }
        fts_upsert_name(&mut tx, &id, name)
            .await
            .map_err(Status::from)?;
        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(&id).await.map_err(Status::from)?;
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        Ok(Response::new(RenameEntityResponse {
            entity: Some(saved),
        }))
    }

    // One property row, not the whole entity, so writers editing different
    // properties of the same entity don't undo each other (I-11). Validates the
    // property with the same checks Create uses: `validate_property` (I-9, I-16)
    // and, through `sync_relation_property`, `check_relation_targets` (I-2).
    // Only a DailyNote's `date` changes the name (and so the FTS name row).
    async fn set_entity_property(
        &self,
        req: Request<SetEntityPropertyRequest>,
    ) -> Result<Response<SetEntityPropertyResponse>, Status> {
        let SetEntityPropertyRequest {
            entity_id,
            property_id,
            value,
        } = req.into_inner();
        if property_id.is_empty() {
            return Err(Status::invalid_argument("property_id is required"));
        }
        reject_reserved_property_id(&property_id)?;
        // An unset value, or one with no case, clears the property.
        let value = value.filter(|v| v.value.is_some());
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;

        let (structure_type, facets) = structure_and_facets(&mut tx, &entity_id).await?;
        let facets: Vec<&str> = facets.iter().map(String::as_str).collect();

        // Clearing is allowed on any property, a facet's included (D6).
        if let Some(v) = &value {
            validate_property(&structure_type, &facets, &property_id, v.value.as_ref())
                .map_err(Status::from)?;
        }

        let previous = sqlx::query_scalar!(
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

        // Only this property's links. The old value counts too, so replacing or
        // clearing an ad-hoc relation property drops its links. Richtext-derived
        // links live under their own source_property_id and aren't touched.
        let value = value.as_ref().and_then(|v| v.value.as_ref());
        let was_relation = is_relation_value(previous.as_ref().and_then(|v| v.value.as_ref()));
        let declared_relation =
            structures::relation_properties(&structure_type).contains(&property_id.as_str())
                || facets.iter().filter_map(|f| structures::facet(f)).any(|f| {
                    structures::facet_relation_properties(f).contains(&property_id.as_str())
                });
        if declared_relation || is_relation_value(value) || was_relation {
            sync_relation_property(
                &mut tx,
                &entity_id,
                &structure_type,
                &property_id,
                value,
                now,
            )
            .await
            .map_err(Status::from)?;
        }

        // The keys this property is the source of. Clearing it drops their rows.
        for def in declared_keys(&structure_type)
            .iter()
            .filter(|k| matches!(k.source, KeySource::Property(id) if id == property_id))
        {
            let key = property_key_value(value);
            let shown = shown_key_value(def.source, "", key.as_deref());
            set_key(
                &mut tx,
                &entity_id,
                &structure_type,
                def.key,
                key.as_deref(),
            )
            .await
            .map_err(|e| map_unique_violation(e, &structure_type, def, shown))?;
        }

        // A DailyNote's `date` drives its name; every other property leaves it
        // be. Clearing the date keeps the name it had.
        if structure_type == "DailyNote" && property_id == "date" {
            let name = daily_note_name(property_key_value(value).as_deref());
            sqlx::query!(
                "UPDATE entities SET name = COALESCE(?, name), updated_at = ? WHERE id = ?",
                name,
                now,
                entity_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
            if let Some(name) = &name {
                fts_upsert_name(&mut tx, &entity_id, name)
                    .await
                    .map_err(Status::from)?;
            }
        } else {
            sqlx::query!(
                "UPDATE entities SET updated_at = ? WHERE id = ?",
                now,
                entity_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }

        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(&entity_id).await.map_err(Status::from)?;
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        Ok(Response::new(SetEntityPropertyResponse {
            entity: Some(saved),
        }))
    }

    /// Delete an entity and everything derived from it, strip its id from other
    /// entities' relation values (I-14), and drop the relationships that point at
    /// it (ADR 12), all in one transaction. After commit it publishes `deleted_id`
    /// first, then one `upserted` per changed entity in id order: those whose
    /// relation values were stripped and those that held a relationship to it, so
    /// no replica keeps the row.
    async fn delete_entity(
        &self,
        req: Request<DeleteEntityRequest>,
    ) -> Result<Response<DeleteEntityResponse>, Status> {
        let id = req.into_inner().id;
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;

        // entity_vec (vec0) can't cascade and keys on chunk ids, so delete the
        // embeddings while the chunks that name them still exist; KNN never
        // returns a tombstone.
        let chunk_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM chunks WHERE entity_id = ?")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::from)?;
        for cid in &chunk_ids {
            sqlx::query("DELETE FROM entity_vec WHERE id = ?")
                .bind(cid)
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;
        }

        // entity_fts (FTS5) can't cascade either. Its row is found through
        // entity_fts_rowids, which the entity delete cascades away, so first.
        fts_delete(&mut tx, &id).await.map_err(Status::from)?;

        // The entities that relate to this one, read before the delete cascades their
        // rows away. They're republished below, so no replica shows the relationship.
        let relationship_sources = sqlx::query_scalar!(
            "SELECT entity_id FROM relationships WHERE target_id = ?",
            id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // ON DELETE CASCADE handles this entity's properties, links, relationships
        // (either end), referenced_dates, keys, FTS rowid, richtext and chunks.
        sqlx::query!("DELETE FROM entities WHERE id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        // Needs the inbound links, so it runs before the sweep below.
        let stripped = strip_relation_refs(&mut tx, &id, now)
            .await
            .map_err(Status::from)?;

        // A source's relationship row is gone, so its `updated_at` moves too.
        for source in &relationship_sources {
            sqlx::query!(
                "UPDATE entities SET updated_at = ? WHERE id = ?",
                now,
                source
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }
        let republish: std::collections::BTreeSet<String> =
            stripped.into_iter().chain(relationship_sources).collect();

        // Inbound links authored by OTHER entities aren't covered by the cascade — sweep them
        // so the relational index never points at a tombstone.
        sqlx::query!("DELETE FROM links WHERE target_id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;
        self.hub
            .publish(watch_entities_response::Event::DeletedId(id));
        for entity_id in &republish {
            let saved = self.load_entity(entity_id).await.map_err(Status::from)?;
            self.hub
                .publish(watch_entities_response::Event::Upserted(saved));
        }
        Ok(Response::new(DeleteEntityResponse {}))
    }

    // Attach a facet (ADR 11): its row and its select defaults, in one
    // transaction. Already attached is a no-op with no write and no event.
    async fn attach_facet(
        &self,
        req: Request<AttachFacetRequest>,
    ) -> Result<Response<AttachFacetResponse>, Status> {
        let AttachFacetRequest { entity_id, facet } = req.into_inner();
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;
        let (structure_type, attached) = structure_and_facets(&mut tx, &entity_id).await?;
        let def = accepted_facet(&structure_type, &facet)?;
        if attached.iter().any(|f| f == def.id) {
            drop(tx);
            let entity = self.load_entity(&entity_id).await.map_err(Status::from)?;
            return Ok(Response::new(AttachFacetResponse {
                entity: Some(entity),
            }));
        }

        sqlx::query!(
            "INSERT INTO entity_facets (entity_id, facet) VALUES (?, ?)",
            entity_id,
            def.id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        // The defaults don't overwrite a value already stored under the facet's
        // namespace (none should be: detach deletes them).
        for (property_id, default) in structures::facet_select_defaults(def) {
            let value = PropertyValue {
                value: Some(property_value::Value::Select(default.to_string())),
            };
            let blob = prost::Message::encode_to_vec(&value);
            sqlx::query!(
                "INSERT INTO properties (entity_id, property_id, value_blob) VALUES (?, ?, ?)
                 ON CONFLICT (entity_id, property_id) DO NOTHING",
                entity_id,
                property_id,
                blob,
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }
        sqlx::query!(
            "UPDATE entities SET updated_at = ? WHERE id = ?",
            now,
            entity_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(&entity_id).await.map_err(Status::from)?;
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        Ok(Response::new(AttachFacetResponse {
            entity: Some(saved),
        }))
    }

    // Detach a facet (ADR 11): its row, every `<facet>.*` property row and the
    // links derived from its relation properties, in one transaction. Not
    // attached is a no-op with no write and no event.
    async fn detach_facet(
        &self,
        req: Request<DetachFacetRequest>,
    ) -> Result<Response<DetachFacetResponse>, Status> {
        let DetachFacetRequest { entity_id, facet } = req.into_inner();
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = begin_write(&self.pool).await.map_err(AppError::from)?;
        let (_, attached) = structure_and_facets(&mut tx, &entity_id).await?;
        let def = known_facet(&facet)?;
        if !attached.iter().any(|f| f == def.id) {
            drop(tx);
            let entity = self.load_entity(&entity_id).await.map_err(Status::from)?;
            return Ok(Response::new(DetachFacetResponse {
                entity: Some(entity),
            }));
        }

        sqlx::query!(
            "DELETE FROM entity_facets WHERE entity_id = ? AND facet = ?",
            entity_id,
            def.id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        // The facet's namespace, `<facet>.`, matched exactly: LIKE would fold
        // ASCII case and read `_` as a wildcard.
        let namespace = format!("{}.", def.id);
        sqlx::query!(
            "DELETE FROM properties
             WHERE entity_id = ?1 AND substr(property_id, 1, length(?2)) = ?2",
            entity_id,
            namespace,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        for property_id in structures::facet_relation_properties(def) {
            sqlx::query!(
                "DELETE FROM links WHERE entity_id = ? AND source_property_id = ?",
                entity_id,
                property_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }
        sqlx::query!(
            "UPDATE entities SET updated_at = ? WHERE id = ?",
            now,
            entity_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(&entity_id).await.map_err(Status::from)?;
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        Ok(Response::new(DetachFacetResponse {
            entity: Some(saved),
        }))
    }

    // Add one target to a `relations` property in one transaction, so writers
    // adding different targets don't overwrite each other's (I-34). Already
    // present is a no-op with no write and no event.
    async fn add_relation_target(
        &self,
        req: Request<AddRelationTargetRequest>,
    ) -> Result<Response<AddRelationTargetResponse>, Status> {
        let AddRelationTargetRequest {
            entity_id,
            property_id,
            target_id,
        } = req.into_inner();
        let entity = self
            .edit_relation_target(RelationTargetOp::Add, &entity_id, &property_id, &target_id)
            .await?;
        Ok(Response::new(AddRelationTargetResponse {
            entity: Some(entity),
        }))
    }

    // Remove one target from a `relations` property, and its link, in one
    // transaction. The last target going deletes the property row. Not present is
    // a no-op with no write and no event.
    async fn remove_relation_target(
        &self,
        req: Request<RemoveRelationTargetRequest>,
    ) -> Result<Response<RemoveRelationTargetResponse>, Status> {
        let RemoveRelationTargetRequest {
            entity_id,
            property_id,
            target_id,
        } = req.into_inner();
        let entity = self
            .edit_relation_target(
                RelationTargetOp::Remove,
                &entity_id,
                &property_id,
                &target_id,
            )
            .await?;
        Ok(Response::new(RemoveRelationTargetResponse {
            entity: Some(entity),
        }))
    }

    // Relate an entity to another (ADR 12): one row in `relationships` and the
    // `@related` links derived from it, in one transaction. Already related is a
    // no-op with no write and no event.
    async fn add_relationship(
        &self,
        req: Request<AddRelationshipRequest>,
    ) -> Result<Response<AddRelationshipResponse>, Status> {
        let AddRelationshipRequest {
            entity_id,
            target_id,
        } = req.into_inner();
        let entity = self
            .edit_relationship(RelationshipOp::Add, &entity_id, &target_id)
            .await?;
        Ok(Response::new(AddRelationshipResponse {
            entity: Some(entity),
        }))
    }

    // Remove a relationship and its link in one transaction. It never consults
    // `relates_to`, so a row a narrowed registry no longer allows can still go. Not
    // related is a no-op with no write and no event.
    async fn remove_relationship(
        &self,
        req: Request<RemoveRelationshipRequest>,
    ) -> Result<Response<RemoveRelationshipResponse>, Status> {
        let RemoveRelationshipRequest {
            entity_id,
            target_id,
        } = req.into_inner();
        let entity = self
            .edit_relationship(RelationshipOp::Remove, &entity_id, &target_id)
            .await?;
        Ok(Response::new(RemoveRelationshipResponse {
            entity: Some(entity),
        }))
    }

    async fn resolve_entity(
        &self,
        req: Request<ResolveEntityRequest>,
    ) -> Result<Response<ResolveEntityResponse>, Status> {
        let ResolveEntityRequest {
            structure_type,
            key,
            create_if_missing,
        } = req.into_inner();
        let resolved = match key {
            Some(resolve_entity_request::Key::Name(name)) => {
                self.resolve_name(&structure_type, &name, create_if_missing)
                    .await?
            }
            Some(resolve_entity_request::Key::Date(date)) => {
                self.resolve_date(&structure_type, &date, create_if_missing)
                    .await?
            }
            None => return Err(Status::invalid_argument("key is required: name or date")),
        };
        Ok(Response::new(resolved))
    }

    async fn list_backlinks(
        &self,
        req: Request<ListBacklinksRequest>,
    ) -> Result<Response<ListBacklinksResponse>, Status> {
        let target_id = req.into_inner().entity_id;
        let exists = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM entities WHERE id = ?"#,
            target_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?
        .is_some();
        if !exists {
            return Err(AppError::NotFound(format!("entity {target_id}")).into());
        }

        // Self-links are left out, matching the frontend's backlinks panel.
        let rows = sqlx::query!(
            r#"SELECT entity_id AS "entity_id!", source_property_id AS "source_property_id!",
                      created_at AS "created_at!"
               FROM links
               WHERE target_id = ?1 AND entity_id != ?1
               ORDER BY created_at DESC, entity_id, link_id"#,
            target_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        let mut source_ids: Vec<String> = rows.iter().map(|r| r.entity_id.clone()).collect();
        source_ids.sort_unstable();
        source_ids.dedup();
        let sources: HashMap<String, Entity> = load_entities_by_id(&self.pool, &source_ids)
            .await?
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect();

        // A source deleted between the two reads has no entity; its rows are skipped.
        let backlinks = rows
            .into_iter()
            .filter_map(|r| {
                Some(Backlink {
                    source: Some(sources.get(&r.entity_id)?.clone()),
                    source_property_id: r.source_property_id,
                    created_at: Some(ts_from_millis(r.created_at)),
                })
            })
            .collect();

        Ok(Response::new(ListBacklinksResponse { backlinks }))
    }

    type WatchEntitiesStream = BoxStream<'static, Result<WatchEntitiesResponse, Status>>;
    async fn watch_entities(
        &self,
        _: Request<WatchEntitiesRequest>,
    ) -> Result<Response<Self::WatchEntitiesStream>, Status> {
        // Subscribe now, before the stream reads a revision (see `watch_stream`).
        let rx = self.hub.subscribe();
        Ok(Response::new(watch_stream(
            self.pool.clone(),
            self.hub.clone(),
            rx,
        )))
    }
}

/// What a Watch stream sends next.
enum WatchPhase {
    /// A snapshot: at the start, and again after the receiver lags.
    Snapshot,
    /// Events newer than the last snapshot.
    Events,
    /// Nothing: a snapshot failed to load and the error has been sent.
    Done,
}

struct WatchState {
    pool: SqlitePool,
    hub: WatchHub,
    rx: broadcast::Receiver<WatchEntitiesResponse>,
    /// The revision the last snapshot is current as of.
    covered: u64,
    phase: WatchPhase,
}

/// The Watch stream (ADR 9): a snapshot of every entity, then each event the
/// snapshot doesn't cover, in revision order. When `rx` lags, the stream sends a
/// fresh snapshot in place of the events it missed. `rx` must already be
/// subscribed to `hub`. The first snapshot is sent as soon as the stream is
/// polled, without waiting for an event.
///
/// Why nothing is lost:
/// 1. `rx` is subscribed before the stream reads `R = hub.current_revision()`,
///    so every event with a revision above R reaches `rx` (or `rx` lags, which
///    starts over with a new snapshot).
/// 2. The snapshot is loaded after reading R. An event with a revision at or
///    below R was published before that, and writes publish only after their
///    transaction commits, so the snapshot already includes it. Skipping it
///    loses nothing.
/// 3. An event above R may repeat what the snapshot already shows, if its write
///    committed before the snapshot query ran. That's harmless: upserts and
///    deletes are idempotent, and the events after it are applied in order.
fn watch_stream(
    pool: SqlitePool,
    hub: WatchHub,
    rx: broadcast::Receiver<WatchEntitiesResponse>,
) -> BoxStream<'static, Result<WatchEntitiesResponse, Status>> {
    let state = WatchState {
        pool,
        hub,
        rx,
        covered: 0,
        phase: WatchPhase::Snapshot,
    };
    Box::pin(futures::stream::unfold(state, |mut s| async move {
        loop {
            match s.phase {
                WatchPhase::Done => return None,
                WatchPhase::Snapshot => {
                    // Read R before loading, per step 2 above.
                    let revision = s.hub.current_revision();
                    return match load_entities(&s.pool, None).await {
                        Ok(entities) => {
                            s.covered = revision;
                            s.phase = WatchPhase::Events;
                            let event = WatchEntitiesResponse {
                                event: Some(watch_entities_response::Event::Snapshot(
                                    EntitySnapshot { entities },
                                )),
                                revision,
                            };
                            Some((Ok(event), s))
                        }
                        Err(e) => {
                            s.phase = WatchPhase::Done;
                            Some((Err(Status::from(e)), s))
                        }
                    };
                }
                WatchPhase::Events => match s.rx.recv().await {
                    Ok(event) if event.revision <= s.covered => continue,
                    Ok(event) => return Some((Ok(event), s)),
                    // Missed events: resync with the same receiver, which now
                    // holds the oldest events still in the channel.
                    Err(RecvError::Lagged(_)) => s.phase = WatchPhase::Snapshot,
                    Err(RecvError::Closed) => return None,
                },
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::rich_text_service_server::RichTextService as _;
    use crate::proto::PutRichTextRequest;
    use crate::test_support::{
        entity_service, memory_pool, note, richtext_service, tag, todo, with_facets, ScratchDb,
    };
    use futures::StreamExt;

    async fn create(svc: &EntityService, req: CreateEntityRequest) -> Entity {
        try_create(svc, req).await.expect("create")
    }

    async fn try_create(svc: &EntityService, req: CreateEntityRequest) -> Result<Entity, Status> {
        svc.create_entity(Request::new(req))
            .await
            .map(|r| r.into_inner().entity.expect("entity"))
    }

    /// A Create request with no name, so the server picks the default.
    fn untitled(structure_type: &str) -> CreateEntityRequest {
        CreateEntityRequest {
            structure_type: structure_type.to_string(),
            name: None,
            properties: HashMap::new(),
            facets: vec![],
        }
    }

    async fn rename(svc: &EntityService, id: &str, name: &str) -> Result<Entity, Status> {
        svc.rename_entity(Request::new(RenameEntityRequest {
            id: id.to_string(),
            name: name.to_string(),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    async fn entity_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM entities")
            .fetch_one(pool)
            .await
            .expect("count entities")
    }

    fn millis(ts: Option<prost_types::Timestamp>) -> i64 {
        let ts = ts.expect("timestamp");
        ts.seconds * 1000 + i64::from(ts.nanos) / 1_000_000
    }

    #[tokio::test]
    async fn create_by_intent_mints_id_defaults_and_name() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let mut rx = svc.hub.subscribe();

        let first = create(&svc, with_facets(untitled("Note"), &["todo"])).await;
        let second = create(&svc, with_facets(untitled("Note"), &["todo"])).await;

        assert!(uuid::Uuid::parse_str(&first.id).is_ok(), "{}", first.id);
        assert_ne!(first.id, second.id);
        assert_eq!(first.structure_type, "Note");
        // Note names aren't unique, so the default may repeat.
        assert_eq!(first.name, "Untitled Note");
        assert_eq!(second.name, "Untitled Note");
        assert_eq!(select_value(&first, "todo.status").as_deref(), Some("open"));
        assert_eq!(
            select_value(&first, "todo.priority").as_deref(),
            Some("none")
        );
        // No stored value for the rich-text `content` (I-22), here or in the DB.
        assert!(!first.properties.contains_key("content"));
        assert!(stored_property_ids(&pool, &first.id)
            .await
            .iter()
            .all(|id| id != "content"));
        // Server-owned fields: no links or dates until content is saved (ADR 3).
        assert!(first.links.is_empty());
        assert!(first.referenced_dates.is_empty());
        assert_eq!(millis(first.created_at), millis(first.updated_at));
        assert_eq!(fts_name(&pool, &first.id).await, "Untitled Note");
        assert_eq!(upserted_ids(&mut rx), [first.id.clone(), second.id.clone()]);

        // A given name is trimmed, and given properties win over the defaults.
        let named = create(
            &svc,
            with_select(todo("  Water plants "), "todo.priority", "high"),
        )
        .await;
        assert_eq!(named.name, "Water plants");
        assert_eq!(
            select_value(&named, "todo.priority").as_deref(),
            Some("high")
        );
        assert_eq!(select_value(&named, "todo.status").as_deref(), Some("open"));

        // A blank name is no name.
        let blank = create(&svc, note("  ")).await;
        assert_eq!(blank.name, "Untitled Note");
    }

    #[tokio::test]
    async fn create_second_untitled_tag_gets_a_free_name() {
        let svc = entity_service(memory_pool().await);
        // Taken case-insensitively, so this one is skipped too.
        create(&svc, tag("UNTITLED TAG 3")).await;

        let names = [
            create(&svc, untitled("Tag")).await.name,
            create(&svc, untitled("Tag")).await.name,
            create(&svc, untitled("Tag")).await.name,
        ];

        assert_eq!(names, ["Untitled Tag", "Untitled Tag 2", "Untitled Tag 4"]);
    }

    #[tokio::test]
    async fn rename_changes_only_the_name() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let urgent = create(&svc, tag("urgent")).await;
        let entity = create(
            &svc,
            with_relations(
                with_select(todo("Before"), "todo.priority", "high"),
                "tags",
                &[(&urgent.id, "Tag")],
            ),
        )
        .await;
        let mut rx = svc.hub.subscribe();

        let renamed = rename(&svc, &entity.id, "After").await.expect("rename");

        assert_eq!(renamed.name, "After");
        // No write can change an entity's type (I-3).
        assert_eq!(renamed.structure_type, "Note");
        assert_eq!(renamed.properties, entity.properties);
        assert_eq!(renamed.links, entity.links);
        assert_eq!(renamed.created_at, entity.created_at);
        assert!(millis(renamed.updated_at) >= millis(entity.updated_at));
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored, renamed);
        assert_eq!(fts_name(&pool, &entity.id).await, "After");
        assert_eq!(upserted_ids(&mut rx), std::slice::from_ref(&entity.id));
    }

    #[tokio::test]
    async fn rename_trims_the_name() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let entity = create(&svc, note("Before")).await;

        let renamed = rename(&svc, &entity.id, "  After \n")
            .await
            .expect("rename");

        assert_eq!(renamed.name, "After");
        assert_eq!(fts_name(&pool, &entity.id).await, "After");
    }

    #[tokio::test]
    async fn rename_to_a_blank_name_is_invalid() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let entity = create(&svc, note("Kept")).await;

        for blank in ["", "   ", "\t\n"] {
            let err = rename(&svc, &entity.id, blank)
                .await
                .expect_err("blank name");
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{blank:?}");
        }
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored, entity);
        assert_eq!(fts_name(&pool, &entity.id).await, "Kept");
    }

    // I-8: an unknown id is NotFound, and nothing is written for it.
    #[tokio::test]
    async fn rename_missing_id_is_not_found() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = rename(&svc, "no-such-entity", "Ghost")
            .await
            .expect_err("rename of a missing id should fail");

        assert_eq!(err.code(), tonic::Code::NotFound);
        let fts_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entity_fts WHERE entity_id = ?")
                .bind("no-such-entity")
                .fetch_one(&pool)
                .await
                .expect("count fts");
        assert_eq!(fts_rows, 0);
    }

    #[tokio::test]
    async fn rename_onto_a_taken_tag_name_is_already_exists() {
        let svc = entity_service(memory_pool().await);
        create(&svc, tag("urgent")).await;
        let other = create(&svc, tag("later")).await;

        let err = rename(&svc, &other.id, "URGENT")
            .await
            .expect_err("tag names are unique");

        assert_eq!(err.code(), tonic::Code::AlreadyExists);
        assert_eq!(err.message(), r#"a Tag named "URGENT" already exists"#);
        let stored = svc.load_entity(&other.id).await.expect("load");
        assert_eq!(stored.name, "later");
    }

    // I-25: the message names the Tag clash, not a DailyNote date.
    #[tokio::test]
    async fn create_onto_a_taken_tag_name_is_already_exists() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        create(&svc, tag("urgent")).await;

        let err = try_create(&svc, tag("Urgent"))
            .await
            .expect_err("tag names are unique");

        assert_eq!(err.code(), tonic::Code::AlreadyExists);
        assert_eq!(err.message(), r#"a Tag named "Urgent" already exists"#);
        assert_eq!(entity_count(&pool).await, 1);
    }

    // ADR 10: a Tag name key folds Unicode case, not just ASCII as NOCASE did.
    #[tokio::test]
    async fn tag_names_differing_in_non_ascii_case_conflict() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        create(&svc, tag("Café")).await;

        let err = try_create(&svc, tag("CAFÉ"))
            .await
            .expect_err("tag names are unique");

        assert_eq!(err.code(), tonic::Code::AlreadyExists);
        assert_eq!(err.message(), r#"a Tag named "CAFÉ" already exists"#);
        assert_eq!(entity_count(&pool).await, 1);
    }

    #[tokio::test]
    async fn rename_moves_a_tag_key_row() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let entity = create(&svc, tag("Reading")).await;
        assert_eq!(
            stored_key(&pool, &entity.id, "name").await.as_deref(),
            Some("reading")
        );

        rename(&svc, &entity.id, " Writing ").await.expect("rename");

        assert_eq!(
            stored_key(&pool, &entity.id, "name").await.as_deref(),
            Some("writing")
        );
        // The old name is free again.
        create(&svc, tag("reading")).await;
        assert_eq!(key_row_count(&pool).await, 2);
    }

    #[tokio::test]
    async fn delete_removes_key_rows() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let label = create(&svc, tag("urgent")).await;
        let day = daily_note(&svc, "2026-06-13").await;
        assert_eq!(key_row_count(&pool).await, 2);

        delete(&svc, &label.id).await;
        delete(&svc, &day.id).await;

        assert_eq!(key_row_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn delete_removes_richtext_chunks_and_embeddings() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let gone = create(&svc, note("Gone")).await;
        let kept = create(&svc, note("Kept")).await;
        for entity in [&gone, &kept] {
            put_content(
                &pool,
                &entity.id,
                &[r#"{"type":"text","text":"body"}"#.to_string()],
            )
            .await;
            // Embeddings are disabled in tests, so store a chunk and its
            // vector as the embed worker would.
            let chunk_id: i64 = sqlx::query_scalar(
                "INSERT INTO chunks (entity_id, chunk_index, text) VALUES (?, 0, '') RETURNING id",
            )
            .bind(&entity.id)
            .fetch_one(&pool)
            .await
            .expect("insert chunk");
            sqlx::query("INSERT INTO entity_vec (id, embedding) VALUES (?, ?)")
                .bind(chunk_id)
                .bind(crate::embed::vector_blob(
                    &[0.0; crate::embed::provider::EMBED_DIM],
                ))
                .execute(&pool)
                .await
                .expect("insert vector");
        }

        delete(&svc, &gone.id).await;

        for (table, sql) in [
            ("richtext", "SELECT entity_id FROM richtext"),
            ("chunks", "SELECT entity_id FROM chunks"),
            (
                "entity_vec",
                "SELECT c.entity_id FROM entity_vec v LEFT JOIN chunks c ON c.id = v.id",
            ),
        ] {
            let owners: Vec<Option<String>> =
                sqlx::query_scalar(sql).fetch_all(&pool).await.expect(table);
            assert_eq!(owners, [Some(kept.id.clone())], "{table}");
        }
    }

    #[tokio::test]
    async fn delete_removes_the_fts_row_and_its_rowid() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let gone = create(&svc, note("Gone")).await;
        let kept = create(&svc, note("Kept")).await;

        delete(&svc, &gone.id).await;

        let fts: Vec<String> = sqlx::query_scalar("SELECT entity_id FROM entity_fts")
            .fetch_all(&pool)
            .await
            .expect("fts rows");
        assert_eq!(fts, vec![kept.id.clone()]);
        let rowids: Vec<String> = sqlx::query_scalar("SELECT entity_id FROM entity_fts_rowids")
            .fetch_all(&pool)
            .await
            .expect("rowid rows");
        assert_eq!(rowids, vec![kept.id.clone()]);
    }

    async fn key_row_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM entity_keys")
            .fetch_one(pool)
            .await
            .expect("count keys")
    }

    // I-26: a DailyNote is named for its date, so Rename refuses it.
    #[tokio::test]
    async fn rename_of_a_daily_note_is_failed_precondition() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let day = resolve(&svc, "", by_date("2026-06-13"), true)
            .await
            .expect("create daily note")
            .entity
            .expect("entity");

        let err = rename(&svc, &day.id, "Friday")
            .await
            .expect_err("DailyNote names aren't editable");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        let stored = svc.load_entity(&day.id).await.expect("load");
        assert_eq!(stored.name, "June 13, 2026");
        assert_eq!(stored.updated_at, day.updated_at);
        assert_eq!(fts_name(&pool, &day.id).await, "June 13, 2026");
    }

    // I-15's done-when: the agent sets a to-do's status while the browser renames
    // it; both edits survive.
    #[tokio::test]
    async fn rename_concurrent_with_set_property_keeps_both() {
        let svc = entity_service(memory_pool().await);
        let snapshot = create(&svc, todo("Water plants")).await;

        set_property(&svc, &snapshot.id, "todo.status", select("done"))
            .await
            .expect("agent sets status");
        rename(&svc, &snapshot.id, "Water the plants")
            .await
            .expect("browser renames");

        let stored = svc.load_entity(&snapshot.id).await.expect("load");
        assert_eq!(stored.name, "Water the plants");
        assert_eq!(
            select_value(&stored, "todo.status").as_deref(),
            Some("done")
        );
    }

    fn select_value(entity: &Entity, property_id: &str) -> Option<String> {
        entity
            .properties
            .get(property_id)
            .and_then(|v| match &v.value {
                Some(property_value::Value::Select(key)) => Some(key.clone()),
                _ => None,
            })
    }

    /// `req` with `property_id` set to `value`, replacing any earlier value.
    fn with_value(
        mut req: CreateEntityRequest,
        property_id: &str,
        value: property_value::Value,
    ) -> CreateEntityRequest {
        req.properties.insert(
            property_id.to_string(),
            PropertyValue { value: Some(value) },
        );
        req
    }

    fn with_select(req: CreateEntityRequest, property_id: &str, key: &str) -> CreateEntityRequest {
        with_value(
            req,
            property_id,
            property_value::Value::Select(key.to_string()),
        )
    }

    fn with_relations(
        req: CreateEntityRequest,
        property_id: &str,
        refs: &[(&str, &str)],
    ) -> CreateEntityRequest {
        let value = tags_value(refs).expect("relations value");
        with_value(req, property_id, value)
    }

    #[tokio::test]
    async fn create_rejects_unknown_select_value() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = try_create(
            &svc,
            with_select(todo("Water plants"), "todo.priority", "urgent"),
        )
        .await
        .expect_err("unknown priority should be rejected");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("urgent"), "{}", err.message());
        assert_eq!(entity_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn select_values_are_checked_against_the_declared_kind() {
        let svc = entity_service(memory_pool().await);

        // A select on a property the structure doesn't declare as one.
        let err = try_create(&svc, with_select(note("Stray"), "status", "open"))
            .await
            .expect_err("select on a non-select property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // A declared select holding some other kind of value.
        let mistyped = with_value(
            todo("Mistyped"),
            "todo.status",
            property_value::Value::Text("open".to_string()),
        );
        let err = try_create(&svc, mistyped)
            .await
            .expect_err("text on a select property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    // I-24: a type the registry doesn't declare can't create, resolve or filter.
    #[tokio::test]
    async fn unknown_structure_types_are_rejected() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        create(&svc, note("Kept")).await;

        for structure_type in ["Custom", ""] {
            let err = try_create(&svc, untitled(structure_type))
                .await
                .expect_err("create of an unknown type");
            assert_eq!(
                err.code(),
                tonic::Code::InvalidArgument,
                "{structure_type:?}"
            );
        }
        let err = resolve(&svc, "Custom", by_name("Anything"), true)
            .await
            .expect_err("resolve of an unknown type");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert_eq!(entity_count(&pool).await, 1);

        let err = svc
            .list_entities(Request::new(ListEntitiesRequest {
                structure_type: "Custom".to_string(),
            }))
            .await
            .expect_err("list of an unknown type");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        // An empty filter still lists every structure.
        let all = svc
            .list_entities(Request::new(ListEntitiesRequest {
                structure_type: String::new(),
            }))
            .await
            .expect("list all")
            .into_inner();
        assert_eq!(all.entities.len(), 1);
    }

    /// (target_id, target_structure) of an entity's links from `property_id`, sorted.
    fn link_targets(entity: &Entity, property_id: &str) -> Vec<(String, String)> {
        let mut targets: Vec<_> = entity
            .links
            .iter()
            .filter(|l| l.source_property_id == property_id)
            .filter_map(|l| l.target.as_ref())
            .map(|t| (t.id.clone(), t.structure_type.clone()))
            .collect();
        targets.sort();
        targets
    }

    #[tokio::test]
    async fn create_rejects_note_in_todo_tags() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let urgent = create(&svc, tag("urgent")).await;
        let other = create(&svc, note("Not a tag")).await;

        let err = try_create(
            &svc,
            with_relations(
                todo("Water plants"),
                "tags",
                &[(&urgent.id, "Tag"), (&other.id, "Note")],
            ),
        )
        .await
        .expect_err("a Note in tags should be rejected");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("tags"), "{}", err.message());
        assert_eq!(entity_count(&pool).await, 2);
    }

    #[tokio::test]
    async fn tag_in_todo_tags_links_with_its_real_type() {
        let svc = entity_service(memory_pool().await);
        let urgent = create(&svc, tag("urgent")).await;

        // An empty claimed type is allowed; the link still records the real one.
        let todo = create(
            &svc,
            with_relations(todo("Water plants"), "tags", &[(&urgent.id, "")]),
        )
        .await;

        assert_eq!(
            link_targets(&todo, "tags"),
            [(urgent.id.clone(), "Tag".to_string())]
        );
    }

    #[tokio::test]
    async fn relation_ref_with_wrong_claimed_type_is_rejected() {
        let svc = entity_service(memory_pool().await);
        let target = create(&svc, note("Target")).await;
        let source = create(&svc, note("Source")).await;

        // Claims a Tag in a declared Tag-only property, but the target is a Note.
        let todo = create(&svc, todo("Water plants")).await;
        let err = set_property(&svc, &todo.id, "tags", tags_value(&[(&target.id, "Tag")]))
            .await
            .expect_err("lying claimed type should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // Ad-hoc properties have no declared target, but the claim is still checked...
        let err = set_property(
            &svc,
            &source.id,
            "related",
            tags_value(&[(&target.id, "Tag")]),
        )
        .await
        .expect_err("lying claimed type on an ad-hoc property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // ...and a truthful one links with the real type.
        let updated = set_property(
            &svc,
            &source.id,
            "related",
            tags_value(&[(&target.id, "Note")]),
        )
        .await
        .expect("ad-hoc relation to any structure");
        assert_eq!(
            link_targets(&updated, "related"),
            [(target.id.clone(), "Note".to_string())]
        );
    }

    // A client may resend a stale tag list that still holds a deleted Tag (Delete
    // strips it from stored values, but not from a client's copy), so a dead ref
    // must not block the write.
    #[tokio::test]
    async fn dead_relation_ref_does_not_block_set_property() {
        let svc = entity_service(memory_pool().await);
        let gone = create(&svc, tag("gone")).await;
        let kept = create(&svc, tag("kept")).await;
        let added = create(&svc, tag("added")).await;
        let todo = create(
            &svc,
            with_relations(
                todo("Water plants"),
                "tags",
                &[(&gone.id, "Tag"), (&kept.id, "Tag")],
            ),
        )
        .await;
        svc.delete_entity(Request::new(DeleteEntityRequest {
            id: gone.id.clone(),
        }))
        .await
        .expect("delete tag");

        let updated = set_property(
            &svc,
            &todo.id,
            "tags",
            tags_value(&[(&gone.id, "Tag"), (&kept.id, "Tag"), (&added.id, "Tag")]),
        )
        .await
        .expect("tag edit with a dead tag ref");

        let mut expected = vec![
            (kept.id.clone(), "Tag".to_string()),
            (added.id.clone(), "Tag".to_string()),
        ];
        expected.sort();
        assert_eq!(link_targets(&updated, "tags"), expected);
    }

    async fn delete(svc: &EntityService, id: &str) {
        svc.delete_entity(Request::new(DeleteEntityRequest { id: id.to_string() }))
            .await
            .expect("delete");
    }

    /// The target ids of a stored `relations` value, in stored order.
    fn relation_ids(entity: &Entity, property_id: &str) -> Option<Vec<String>> {
        match entity.properties.get(property_id)?.value.as_ref()? {
            property_value::Value::Relations(list) => {
                Some(list.refs.iter().map(|r| r.id.clone()).collect())
            }
            other => panic!("expected relations, got {other:?}"),
        }
    }

    /// A to-do tagged with `tags`, created a few milliseconds in the past so a
    /// later bump of `updated_at` is visible.
    async fn tagged_todo(svc: &EntityService, tags: &[&Entity]) -> Entity {
        let refs: Vec<(&str, &str)> = tags.iter().map(|t| (t.id.as_str(), "Tag")).collect();
        let todo = create(svc, with_relations(todo("Water plants"), "tags", &refs)).await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        todo
    }

    #[tokio::test]
    async fn delete_strips_the_id_from_relations_values() {
        let svc = entity_service(memory_pool().await);
        let gone = create(&svc, tag("gone")).await;
        let kept = create(&svc, tag("kept")).await;
        let todo = tagged_todo(&svc, &[&gone, &kept]).await;

        delete(&svc, &gone.id).await;

        let stored = svc.load_entity(&todo.id).await.expect("load");
        assert_eq!(relation_ids(&stored, "tags"), Some(vec![kept.id.clone()]));
        assert_eq!(
            link_targets(&stored, "tags"),
            [(kept.id.clone(), "Tag".to_string())]
        );
        assert!(millis(stored.updated_at) > millis(todo.updated_at));
    }

    #[tokio::test]
    async fn delete_clears_a_relations_value_left_empty() {
        let svc = entity_service(memory_pool().await);
        let only = create(&svc, tag("only")).await;
        let todo = tagged_todo(&svc, &[&only]).await;

        delete(&svc, &only.id).await;

        let stored = svc.load_entity(&todo.id).await.expect("load");
        assert!(!stored.properties.contains_key("tags"));
        assert!(link_targets(&stored, "tags").is_empty());
        assert!(millis(stored.updated_at) > millis(todo.updated_at));
    }

    #[tokio::test]
    async fn delete_clears_a_single_relation_value() {
        let svc = entity_service(memory_pool().await);
        let target = create(&svc, note("Target")).await;
        let source = create(&svc, note("Source")).await;
        let source = set_property(
            &svc,
            &source.id,
            "parent",
            Some(property_value::Value::Relation(EntityRef {
                id: target.id.clone(),
                structure_type: String::new(),
            })),
        )
        .await
        .expect("set ad-hoc relation");
        assert_eq!(link_targets(&source, "parent").len(), 1);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        delete(&svc, &target.id).await;

        let stored = svc.load_entity(&source.id).await.expect("load");
        assert!(!stored.properties.contains_key("parent"));
        assert!(stored.links.is_empty());
        assert!(millis(stored.updated_at) > millis(source.updated_at));
    }

    #[tokio::test]
    async fn delete_publishes_upserts_for_stripped_entities() {
        let svc = entity_service(memory_pool().await);
        let gone = create(&svc, tag("gone")).await;
        let kept = create(&svc, tag("kept")).await;
        let todo = tagged_todo(&svc, &[&gone, &kept]).await;
        let mut rx = svc.hub.subscribe();

        delete(&svc, &gone.id).await;

        let first = rx.recv().await.expect("deleted event");
        assert_eq!(
            first.event,
            Some(watch_entities_response::Event::DeletedId(gone.id.clone()))
        );
        let second = rx.recv().await.expect("upserted event");
        let Some(watch_entities_response::Event::Upserted(upserted)) = second.event else {
            panic!("expected an upsert, got {second:?}");
        };
        assert_eq!(upserted.id, todo.id);
        assert_eq!(relation_ids(&upserted, "tags"), Some(vec![kept.id.clone()]));
        assert!(rx.try_recv().is_err(), "no further events");
    }

    #[tokio::test]
    async fn delete_leaves_rich_text_mentions_out_of_properties() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let gone = create(&svc, tag("gone")).await;
        let mentioner = create(&svc, todo("Mentions the tag")).await;
        let doc = format!(
            r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"mention","attrs":{{"id":"{}","structureType":"Tag"}}}}]}}]}}"#,
            gone.id
        );
        richtext_service(pool.clone(), WatchHub::new())
            .put_rich_text(Request::new(PutRichTextRequest {
                entity_id: mentioner.id.clone(),
                property_id: "content".to_string(),
                doc,
                expected_updated_at: None,
            }))
            .await
            .expect("put content");
        let before = svc.load_entity(&mentioner.id).await.expect("load");
        assert_eq!(link_targets(&before, "content").len(), 1);
        let mut rx = svc.hub.subscribe();

        delete(&svc, &gone.id).await;

        let after = svc.load_entity(&mentioner.id).await.expect("load");
        assert_eq!(after.properties, before.properties);
        assert_eq!(after.updated_at, before.updated_at);
        let event = rx.recv().await.expect("deleted event");
        assert_eq!(
            event.event,
            Some(watch_entities_response::Event::DeletedId(gone.id.clone()))
        );
        assert!(rx.try_recv().is_err(), "no upsert for a rich-text mention");
    }

    async fn set_property(
        svc: &EntityService,
        entity_id: &str,
        property_id: &str,
        value: Option<property_value::Value>,
    ) -> Result<Entity, Status> {
        svc.set_entity_property(Request::new(SetEntityPropertyRequest {
            entity_id: entity_id.to_string(),
            property_id: property_id.to_string(),
            value: value.map(|value| PropertyValue { value: Some(value) }),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    fn select(key: &str) -> Option<property_value::Value> {
        Some(property_value::Value::Select(key.to_string()))
    }

    fn tags_value(refs: &[(&str, &str)]) -> Option<property_value::Value> {
        Some(property_value::Value::Relations(EntityRefList {
            refs: refs
                .iter()
                .map(|(id, structure_type)| EntityRef {
                    id: id.to_string(),
                    structure_type: structure_type.to_string(),
                })
                .collect(),
        }))
    }

    // I-11's done-when: two writers start from the same snapshot and each edit a
    // different property; both edits survive.
    #[tokio::test]
    async fn concurrent_set_property_edits_both_survive() {
        let svc = entity_service(memory_pool().await);
        let snapshot = create(&svc, todo("Water plants")).await;

        set_property(&svc, &snapshot.id, "todo.status", select("done"))
            .await
            .expect("writer A");
        set_property(&svc, &snapshot.id, "todo.priority", select("high"))
            .await
            .expect("writer B");

        let stored = svc.load_entity(&snapshot.id).await.expect("load");
        assert_eq!(
            select_value(&stored, "todo.status").as_deref(),
            Some("done")
        );
        assert_eq!(
            select_value(&stored, "todo.priority").as_deref(),
            Some("high")
        );
    }

    // I-40: two writers race on one entity. Each reads before it writes, so a
    // DEFERRED transaction that loses would fail at once with SQLITE_BUSY
    // (Internal) instead of waiting on busy_timeout. Both must succeed. A few
    // rounds, because one race can miss the window.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn racing_set_property_writes_both_succeed() {
        let scratch = ScratchDb::new().await;
        let svc = entity_service(scratch.pool.clone());
        let entity = create(&svc, todo("Water plants")).await;

        for (status, priority) in [("done", "high"), ("open", "low"), ("done", "medium")] {
            let (a, b) = tokio::join!(
                set_property(&svc, &entity.id, "todo.status", select(status)),
                set_property(&svc, &entity.id, "todo.priority", select(priority)),
            );
            a.expect("status write");
            b.expect("priority write");

            let stored = svc.load_entity(&entity.id).await.expect("load");
            assert_eq!(
                select_value(&stored, "todo.status").as_deref(),
                Some(status)
            );
            assert_eq!(
                select_value(&stored, "todo.priority").as_deref(),
                Some(priority)
            );
        }
        scratch.pool.close().await;
    }

    #[tokio::test]
    async fn set_property_rejects_unknown_select_value() {
        let svc = entity_service(memory_pool().await);
        let entity = create(&svc, todo("Water plants")).await;

        let err = set_property(&svc, &entity.id, "todo.status", select("banana"))
            .await
            .expect_err("unknown status should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("banana"), "{}", err.message());

        // A select on a property not declared as one is rejected too.
        let err = set_property(&svc, &entity.id, "todo.due", select("open"))
            .await
            .expect_err("select on a non-select property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored.properties, entity.properties);
        assert_eq!(stored.updated_at, entity.updated_at);
    }

    // I-16: every declared property takes only its declared kind.
    #[tokio::test]
    async fn values_must_match_the_declared_kind() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let entity = create(&svc, todo("Water plants")).await;

        // A `date` value on a select, through SetProperty and Create.
        let err = set_property(&svc, &entity.id, "todo.status", date("2026-06-13"))
            .await
            .expect_err("date on a select");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let err = try_create(
            &svc,
            with_value(
                todo("Dated"),
                "todo.priority",
                property_value::Value::Date("2026-06-13".to_string()),
            ),
        )
        .await
        .expect_err("date on a select");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // A `text` value on a declared date.
        let err = set_property(
            &svc,
            &entity.id,
            "todo.due",
            Some(property_value::Value::Text("tomorrow".to_string())),
        )
        .await
        .expect_err("text on a date property");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored.properties, entity.properties);
        assert_eq!(stored.links, entity.links);
        assert_eq!(entity_count(&pool).await, 1);

        // The matching kinds are accepted: a date on `due`, and anything on an
        // undeclared id.
        set_property(&svc, &entity.id, "todo.due", date("2026-06-13"))
            .await
            .expect("date on due");
        set_property(
            &svc,
            &entity.id,
            "mood",
            Some(property_value::Value::Number(3.0)),
        )
        .await
        .expect("undeclared property is unchecked");
    }

    // I-22: a rich-text property's document is addressed by (entity id, declared
    // property id), so the property itself takes no value of any kind.
    #[tokio::test]
    async fn rich_text_properties_take_no_value() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let target = create(&svc, note("Target")).await;
        let entity = create(&svc, todo("Water plants")).await;

        for value in [
            property_value::Value::Text("hello".to_string()),
            tags_value(&[(&target.id, "Note")]).expect("relations value"),
        ] {
            let err = set_property(&svc, &entity.id, "content", Some(value.clone()))
                .await
                .expect_err("SetProperty value on a rich-text property");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
            assert!(err.message().contains("rich-text"), "{}", err.message());

            let err = try_create(&svc, with_value(note("Sneaky"), "content", value))
                .await
                .expect_err("Create value on a rich-text property");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
            assert!(err.message().contains("rich-text"), "{}", err.message());
        }

        assert_eq!(entity_count(&pool).await, 2);
        assert!(stored_property_ids(&pool, &entity.id)
            .await
            .iter()
            .all(|id| id != "content"));
        assert!(stored_property_ids(&pool, &target.id).await.is_empty());
        // Clearing it is a harmless no-op, as on any property.
        set_property(&svc, &entity.id, "content", None)
            .await
            .expect("clear content");
    }

    #[tokio::test]
    async fn set_property_checks_relation_targets() {
        let svc = entity_service(memory_pool().await);
        let urgent = create(&svc, tag("urgent")).await;
        let other = create(&svc, note("Not a tag")).await;
        let entity = create(&svc, todo("Water plants")).await;

        let err = set_property(
            &svc,
            &entity.id,
            "tags",
            tags_value(&[(&urgent.id, "Tag"), (&other.id, "Note")]),
        )
        .await
        .expect_err("a Note in tags should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("tags"), "{}", err.message());
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored.properties, entity.properties);
        assert!(link_targets(&stored, "tags").is_empty());

        // An empty claimed type is allowed; the link records the real one.
        let updated = set_property(&svc, &entity.id, "tags", tags_value(&[(&urgent.id, "")]))
            .await
            .expect("a Tag in tags");
        assert_eq!(
            link_targets(&updated, "tags"),
            [(urgent.id.clone(), "Tag".to_string())]
        );
    }

    #[tokio::test]
    async fn set_property_clear_removes_only_that_property_and_its_links() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let urgent = create(&svc, tag("urgent")).await;
        let mentioned = create(&svc, note("Mentioned")).await;
        let entity = create(
            &svc,
            with_relations(todo("Water plants"), "tags", &[(&urgent.id, "Tag")]),
        )
        .await;
        // Content that mentions a note, for content-derived links to keep.
        let doc = format!(
            r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"mention","attrs":{{"id":"{}","structureType":"Note"}}}}]}}]}}"#,
            mentioned.id
        );
        richtext_service(pool.clone(), WatchHub::new())
            .put_rich_text(Request::new(PutRichTextRequest {
                entity_id: entity.id.clone(),
                property_id: "content".to_string(),
                doc,
                expected_updated_at: None,
            }))
            .await
            .expect("put content");
        let entity = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(link_targets(&entity, "tags").len(), 1);
        assert_eq!(link_targets(&entity, "content").len(), 1);

        let cleared = set_property(&svc, &entity.id, "tags", None)
            .await
            .expect("clear tags");

        assert!(!cleared.properties.contains_key("tags"));
        assert!(link_targets(&cleared, "tags").is_empty());
        let mut expected = entity.properties.clone();
        expected.remove("tags");
        assert_eq!(cleared.properties, expected);
        assert_eq!(
            link_targets(&cleared, "content"),
            [(mentioned.id.clone(), "Note".to_string())]
        );

        // A value with no case clears too.
        set_property(&svc, &entity.id, "todo.status", None)
            .await
            .expect("clear status");
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(select_value(&stored, "todo.status"), None);
        assert_eq!(
            select_value(&stored, "todo.priority").as_deref(),
            Some("none")
        );
    }

    #[tokio::test]
    async fn set_property_missing_entity_is_not_found() {
        let svc = entity_service(memory_pool().await);

        let err = set_property(&svc, "no-such-entity", "todo.status", select("done"))
            .await
            .expect_err("missing entity");

        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    /// The DailyNote for `date`, created through Resolve (DailyNote isn't creatable).
    async fn daily_note(svc: &EntityService, date: &str) -> Entity {
        resolve(svc, "", by_date(date), true)
            .await
            .expect("resolve daily note")
            .entity
            .expect("entity")
    }

    fn date(d: &str) -> Option<property_value::Value> {
        Some(property_value::Value::Date(d.to_string()))
    }

    async fn fts_name(pool: &SqlitePool, entity_id: &str) -> String {
        sqlx::query_scalar("SELECT name FROM entity_fts WHERE entity_id = ?")
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .expect("fts row")
    }

    /// The property ids stored in `properties` for `entity_id`, sorted.
    async fn stored_property_ids(pool: &SqlitePool, entity_id: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT property_id FROM properties WHERE entity_id = ? ORDER BY property_id",
        )
        .bind(entity_id)
        .fetch_all(pool)
        .await
        .expect("property rows")
    }

    /// The entity's stored `entity_keys` value for `key`, if any.
    async fn stored_key(pool: &SqlitePool, entity_id: &str, key: &str) -> Option<String> {
        sqlx::query_scalar("SELECT value FROM entity_keys WHERE entity_id = ? AND key = ?")
            .bind(entity_id)
            .bind(key)
            .fetch_optional(pool)
            .await
            .expect("key row")
    }

    async fn stored_date_key(pool: &SqlitePool, entity_id: &str) -> Option<String> {
        stored_key(pool, entity_id, "date").await
    }

    #[test]
    fn new_entity_builds_defaults_and_lets_the_caller_win() {
        let entity = new_entity(
            "Note",
            Some("Water plants"),
            HashMap::from([(
                "todo.priority".to_string(),
                PropertyValue {
                    value: Some(property_value::Value::Select("high".to_string())),
                },
            )]),
            &[structures::facet("todo").expect("todo facet")],
        );

        assert_eq!(entity.name, "Water plants");
        assert_eq!(
            select_value(&entity, "todo.status").as_deref(),
            Some("open")
        );
        assert_eq!(
            select_value(&entity, "todo.priority").as_deref(),
            Some("high")
        );
        // The defaults (status, priority) with the caller's priority laid over.
        assert_eq!(entity.properties.len(), 2);
        // Rich-text properties get no value (I-22).
        assert!(!entity.properties.contains_key("content"));
    }

    // A map entry can't omit its value, so a value with no case is the "missing
    // value" a Property with no value used to be (I-28).
    #[tokio::test]
    async fn create_with_an_empty_property_value_is_invalid_argument() {
        let svc = entity_service(memory_pool().await);
        let mut req = note("Empty");
        req.properties
            .insert("mood".to_string(), PropertyValue { value: None });

        let err = try_create(&svc, req).await.expect_err("empty value");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn new_entity_names_a_daily_note_for_its_date() {
        let entity = new_entity(
            "DailyNote",
            Some("Ignored"),
            date_properties("2026-06-13"),
            &[],
        );
        assert_eq!(entity.name, "June 13, 2026");
    }

    // I-26: DailyNote isn't creatable; Create points at Resolve, which still
    // creates it by date.
    #[tokio::test]
    async fn create_of_a_daily_note_is_failed_precondition() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = try_create(
            &svc,
            CreateEntityRequest {
                structure_type: "DailyNote".to_string(),
                name: None,
                properties: date_properties("2026-06-13"),
                facets: vec![],
            },
        )
        .await
        .expect_err("DailyNote isn't creatable");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("Resolve"), "{}", err.message());
        assert_eq!(entity_count(&pool).await, 0);

        let day = daily_note(&svc, "2026-06-13").await;
        assert_eq!(day.name, "June 13, 2026");
    }

    // I-52: a date value is a real day in canonical form, declared (todo.due,
    // DailyNote.date) or ad hoc (a Note's `when`), on Create and SetProperty.
    #[tokio::test]
    async fn date_values_must_be_iso_days() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let task = create(&svc, todo("Task")).await;
        let memo = create(&svc, note("Memo")).await;
        let day = daily_note(&svc, "2026-06-13").await;
        let targets = [
            (&task, "todo.due", todo("T")),
            (&memo, "when", note("M")),
            (&day, "date", note("D")),
        ];

        for bad in ["2026-13-45", "2026-2-3", "June 13", ""] {
            let value = property_value::Value::Date(bad.to_string());
            for (entity, property_id, req) in &targets {
                if entity.structure_type != "DailyNote" {
                    let err = try_create(&svc, with_value(req.clone(), property_id, value.clone()))
                        .await
                        .expect_err("bad date on create");
                    assert_eq!(err.code(), tonic::Code::InvalidArgument, "create {bad:?}");
                }
                let err = set_property(&svc, &entity.id, property_id, Some(value.clone()))
                    .await
                    .expect_err("bad date on set");
                assert_eq!(err.code(), tonic::Code::InvalidArgument, "set {bad:?}");
            }
        }
        assert_eq!(entity_count(&pool).await, 3);
        assert_eq!(
            stored_date_key(&pool, &day.id).await.as_deref(),
            Some("2026-06-13")
        );

        let good = property_value::Value::Date("2026-02-03".to_string());
        try_create(&svc, with_value(todo("T"), "todo.due", good.clone()))
            .await
            .expect("declared date on create");
        try_create(&svc, with_value(note("M"), "when", good.clone()))
            .await
            .expect("ad-hoc date on create");
        for (entity, property_id, _) in &targets {
            set_property(&svc, &entity.id, property_id, Some(good.clone()))
                .await
                .expect("good date on set");
        }
        assert_eq!(
            stored_date_key(&pool, &day.id).await.as_deref(),
            Some("2026-02-03")
        );
    }

    #[tokio::test]
    async fn set_property_date_renames_daily_note() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let note = daily_note(&svc, "2026-06-13").await;

        let moved = set_property(&svc, &note.id, "date", date("2026-06-15"))
            .await
            .expect("move to a free day");

        assert_eq!(moved.name, "June 15, 2026");
        assert_eq!(fts_name(&pool, &note.id).await, "June 15, 2026");
        assert_eq!(
            stored_date_key(&pool, &note.id).await.as_deref(),
            Some("2026-06-15")
        );

        // Clearing the date keeps the name it had.
        let cleared = set_property(&svc, &note.id, "date", None)
            .await
            .expect("clear date");
        assert_eq!(cleared.name, "June 15, 2026");
        assert_eq!(stored_date_key(&pool, &note.id).await, None);
    }

    #[tokio::test]
    async fn set_property_date_onto_existing_day_is_already_exists() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        daily_note(&svc, "2026-06-13").await;
        let second = daily_note(&svc, "2026-06-14").await;

        let err = set_property(&svc, &second.id, "date", date("2026-06-13"))
            .await
            .expect_err("day already has a note");

        assert_eq!(err.code(), tonic::Code::AlreadyExists);
        assert_eq!(err.message(), "a DailyNote for 2026-06-13 already exists");
        let stored = svc.load_entity(&second.id).await.expect("load");
        assert_eq!(stored.name, "June 14, 2026");
        assert_eq!(stored.properties, second.properties);
        assert_eq!(fts_name(&pool, &second.id).await, "June 14, 2026");
        assert_eq!(
            stored_date_key(&pool, &second.id).await.as_deref(),
            Some("2026-06-14")
        );
    }

    #[tokio::test]
    async fn set_property_moves_a_daily_note_date_key() {
        let svc = entity_service(memory_pool().await);
        let first = daily_note(&svc, "2026-06-13").await;
        let second = daily_note(&svc, "2026-06-14").await;

        set_property(&svc, &first.id, "date", date("2026-06-15"))
            .await
            .expect("move to a free day");
        // The old day is free again; the new one is taken.
        let resolved = resolve(&svc, "", by_date("2026-06-13"), true)
            .await
            .expect("old day is free");
        assert!(resolved.created);
        let err = set_property(&svc, &second.id, "date", date("2026-06-15"))
            .await
            .expect_err("day already has a note");
        assert_eq!(err.code(), tonic::Code::AlreadyExists);
    }

    #[tokio::test]
    async fn clearing_a_daily_note_date_removes_its_key_row() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let day = daily_note(&svc, "2026-06-13").await;

        set_property(&svc, &day.id, "date", None)
            .await
            .expect("clear date");

        assert_eq!(stored_date_key(&pool, &day.id).await, None);
        assert_eq!(key_row_count(&pool).await, 0);
        let resolved = resolve(&svc, "", by_date("2026-06-13"), true)
            .await
            .expect("the day is free");
        assert!(resolved.created);
    }

    async fn resolve(
        svc: &EntityService,
        structure_type: &str,
        key: resolve_entity_request::Key,
        create_if_missing: bool,
    ) -> Result<ResolveEntityResponse, Status> {
        svc.resolve_entity(Request::new(ResolveEntityRequest {
            structure_type: structure_type.to_string(),
            key: Some(key),
            create_if_missing,
        }))
        .await
        .map(Response::into_inner)
    }

    fn by_date(d: &str) -> resolve_entity_request::Key {
        resolve_entity_request::Key::Date(d.to_string())
    }

    fn by_name(name: &str) -> resolve_entity_request::Key {
        resolve_entity_request::Key::Name(name.to_string())
    }

    /// The ids of the entities Upserted on `rx` so far.
    fn upserted_ids(
        rx: &mut tokio::sync::broadcast::Receiver<WatchEntitiesResponse>,
    ) -> Vec<String> {
        let mut ids = vec![];
        while let Ok(event) = rx.try_recv() {
            if let Some(watch_entities_response::Event::Upserted(e)) = event.event {
                ids.push(e.id);
            }
        }
        ids
    }

    #[tokio::test]
    async fn resolve_by_date_gets_or_creates() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let mut rx = svc.hub.subscribe();

        let first = resolve(&svc, "", by_date("2026-06-13"), true)
            .await
            .expect("create");
        assert!(first.created);
        let entity = first.entity.expect("entity");
        assert_eq!(entity.structure_type, "DailyNote");
        assert_eq!(entity.name, "June 13, 2026");
        assert_eq!(
            stored_date_key(&pool, &entity.id).await.as_deref(),
            Some("2026-06-13")
        );
        // No stored value for its rich-text `content` (I-22).
        assert!(!entity.properties.contains_key("content"));
        assert_eq!(upserted_ids(&mut rx), std::slice::from_ref(&entity.id));

        // Again, with the structure named: the same note, nothing published.
        let again = resolve(&svc, "DailyNote", by_date("2026-06-13"), true)
            .await
            .expect("get");
        assert!(!again.created);
        assert_eq!(again.entity.expect("entity").id, entity.id);
        assert!(upserted_ids(&mut rx).is_empty());
    }

    #[tokio::test]
    async fn resolve_by_date_without_create_is_not_found() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = resolve(&svc, "", by_date("2026-06-13"), false)
            .await
            .expect_err("no note for that day");

        assert_eq!(err.code(), tonic::Code::NotFound);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities")
            .fetch_one(&pool)
            .await
            .expect("count entities");
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn resolve_rejects_malformed_requests() {
        let svc = entity_service(memory_pool().await);

        for (structure_type, key) in [
            ("Note", by_date("2026-06-13")),
            ("", by_date("2026-6-13")),
            ("", by_date("June 13")),
            ("", by_date("")),
            ("", by_name("")),
            ("Note", by_name("  ")),
            ("", by_name("Orphan")),
        ] {
            let err = resolve(&svc, structure_type, key.clone(), true)
                .await
                .expect_err("bad request");
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{key:?}");
        }
        let err = svc
            .resolve_entity(Request::new(ResolveEntityRequest {
                structure_type: "Note".to_string(),
                key: None,
                create_if_missing: true,
            }))
            .await
            .expect_err("no key");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn resolve_by_name_gets_or_creates() {
        let svc = entity_service(memory_pool().await);
        let mut rx = svc.hub.subscribe();

        let first = resolve(&svc, "Note", by_name("  Water plants "), true)
            .await
            .expect("create");
        assert!(first.created);
        let entity = first.entity.expect("entity");
        assert_eq!(entity.structure_type, "Note");
        assert_eq!(entity.name, "Water plants");
        assert_eq!(upserted_ids(&mut rx), std::slice::from_ref(&entity.id));

        // Case-insensitive, and scoped to the structure.
        let again = resolve(&svc, "Note", by_name("WATER PLANTS"), true)
            .await
            .expect("get");
        assert!(!again.created);
        assert_eq!(again.entity.expect("entity").id, entity.id);
        assert!(upserted_ids(&mut rx).is_empty());

        let tag = resolve(&svc, "Tag", by_name("Water plants"), true)
            .await
            .expect("a Tag of the same name");
        assert!(tag.created);
        assert_ne!(tag.entity.expect("entity").id, entity.id);
    }

    // I-24, D2: with two same-named Notes, the oldest wins, then the lower id.
    #[tokio::test]
    async fn resolve_by_name_with_duplicates_returns_the_oldest() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("Same")).await;
        let b = create(&svc, note("same")).await;
        let (low, high) = if a.id < b.id { (&a, &b) } else { (&b, &a) };
        let set_created_at = |id: String, at: i64| {
            let pool = pool.clone();
            async move {
                sqlx::query("UPDATE entities SET created_at = ? WHERE id = ?")
                    .bind(at)
                    .bind(id)
                    .execute(&pool)
                    .await
                    .expect("set created_at");
            }
        };
        let resolved_id = || async {
            resolve(&svc, "Note", by_name("SAME"), false)
                .await
                .expect("resolve")
                .entity
                .expect("entity")
                .id
        };

        // The higher id is older, so it wins over the lower one.
        set_created_at(high.id.clone(), 1_000).await;
        set_created_at(low.id.clone(), 2_000).await;
        for _ in 0..3 {
            assert_eq!(resolved_id().await, high.id);
        }

        // Same created_at: the lower id breaks the tie.
        set_created_at(high.id.clone(), 2_000).await;
        for _ in 0..3 {
            assert_eq!(resolved_id().await, low.id);
        }
    }

    // A Tag is found by its folded name key, so any casing finds it.
    #[tokio::test]
    async fn resolve_tag_by_name_ignores_case() {
        let svc = entity_service(memory_pool().await);
        let reading = create(&svc, tag("Reading")).await;
        let cafe = create(&svc, tag("Café")).await;

        for (name, want) in [
            ("reading", &reading),
            (" READING ", &reading),
            ("café", &cafe),
            ("CAFÉ", &cafe),
        ] {
            let found = resolve(&svc, "Tag", by_name(name), true)
                .await
                .expect("resolve");
            assert!(!found.created, "{name:?}");
            assert_eq!(found.entity.expect("entity").id, want.id, "{name:?}");
        }
    }

    #[tokio::test]
    async fn resolve_by_name_without_create_is_not_found() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        create(&svc, tag("ml")).await;

        let err = resolve(&svc, "Note", by_name("ml"), false)
            .await
            .expect_err("no Note named ml");

        assert_eq!(err.code(), tonic::Code::NotFound);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities")
            .fetch_one(&pool)
            .await
            .expect("count entities");
        assert_eq!(rows, 1);
    }

    // I-46: DailyNote isn't creatable, so Resolve by name can only look one up.
    #[tokio::test]
    async fn resolve_by_name_of_a_missing_daily_note_is_failed_precondition() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = resolve(&svc, "DailyNote", by_name("Someday"), true)
            .await
            .expect_err("DailyNote isn't creatable");
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("by date"), "{}", err.message());
        assert_eq!(entity_count(&pool).await, 0);

        let err = resolve(&svc, "DailyNote", by_name("Someday"), false)
            .await
            .expect_err("no such DailyNote");
        assert_eq!(err.code(), tonic::Code::NotFound);

        // A lookup of an existing one still works.
        let day = daily_note(&svc, "2026-06-13").await;
        let found = resolve(&svc, "DailyNote", by_name("june 13, 2026"), true)
            .await
            .expect("lookup");
        assert!(!found.created);
        assert_eq!(found.entity.expect("entity").id, day.id);
    }

    /// Put `content` on `entity_id` as a paragraph of the given inline nodes.
    async fn put_content(pool: &SqlitePool, entity_id: &str, nodes: &[String]) {
        let doc = format!(
            r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{}]}}]}}"#,
            nodes.join(",")
        );
        richtext_service(pool.clone(), WatchHub::new())
            .put_rich_text(Request::new(PutRichTextRequest {
                entity_id: entity_id.to_string(),
                property_id: "content".to_string(),
                doc,
                expected_updated_at: None,
            }))
            .await
            .expect("put content");
    }

    fn mention(node_type: &str, target: &Entity) -> String {
        format!(
            r#"{{"type":"{node_type}","attrs":{{"id":"{}","structureType":"{}"}}}}"#,
            target.id, target.structure_type
        )
    }

    fn date_chip(date: &str) -> String {
        format!(r#"{{"type":"dateChip","attrs":{{"date":"{date}"}}}}"#)
    }

    /// Ids in List's order before batching: the query List ran for its ids.
    async fn listed_ids(pool: &SqlitePool, structure_type: Option<&str>) -> Vec<String> {
        match structure_type {
            None => sqlx::query_scalar("SELECT id FROM entities ORDER BY updated_at DESC, id")
                .fetch_all(pool)
                .await
                .expect("ids"),
            Some(s) => sqlx::query_scalar(
                "SELECT id FROM entities WHERE structure_type = ? ORDER BY updated_at DESC, id",
            )
            .bind(s)
            .fetch_all(pool)
            .await
            .expect("ids"),
        }
    }

    #[tokio::test]
    async fn load_entities_matches_load_entity() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let urgent = create(&svc, tag("urgent")).await;
        let home = create(&svc, tag("home")).await;
        let other = create(&svc, note("Other")).await;
        let plants = create(
            &svc,
            with_relations(
                todo("Water plants"),
                "tags",
                &[(&urgent.id, "Tag"), (&home.id, "Tag")],
            ),
        )
        .await;
        let _bare = create(&svc, todo("Bare")).await;
        let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
        let ideas = create(&svc, note("Ideas")).await;
        let day = daily_note(&svc, "2026-06-15").await;
        // Several links and dates per entity, dates written out of order, so the
        // ordering inside an entity is exercised.
        put_content(
            &pool,
            &ideas.id,
            &[
                mention("mention", &plants),
                mention("hashtag", &urgent),
                mention("mention", &other),
                date_chip("2026-09-01"),
                date_chip("2026-01-02"),
            ],
        )
        .await;
        put_content(
            &pool,
            &plants.id,
            &[mention("mention", &ideas), date_chip("2026-07-04")],
        )
        .await;
        put_content(
            &pool,
            &day.id,
            &[
                mention("hashtag", &home),
                date_chip("2026-12-25"),
                date_chip("2026-03-03"),
            ],
        )
        .await;

        for filter in [None, Some("Note"), Some("Tag"), Some("DailyNote")] {
            let mut expected = Vec::new();
            for id in listed_ids(&pool, filter).await {
                expected.push(load_entity(&pool, &id).await.expect("load"));
            }
            let batched = load_entities(&pool, filter).await.expect("load entities");
            assert_eq!(batched, expected, "filter {filter:?}");
        }
        let all = load_entities(&pool, None).await.expect("load entities");
        assert_eq!(all.len(), 8);
        let batched_ship = all.iter().find(|e| e.id == ship.id).expect("ship listed");
        assert_eq!(batched_ship.facets, vec!["todo".to_string()]);

        // By id: only the named entities that exist, each as `load_entity` has it.
        let ids = [
            ideas.id.clone(),
            "missing".to_string(),
            plants.id.clone(),
            day.id.clone(),
        ];
        let by_id = load_entities_by_id(&pool, &ids).await.expect("load by id");
        let mut expected: Vec<&Entity> = all
            .iter()
            .filter(|e| [&ideas.id, &plants.id, &day.id].contains(&&e.id))
            .collect();
        expected.sort_by_key(|e| e.id.clone());
        let mut got: Vec<&Entity> = by_id.iter().collect();
        got.sort_by_key(|e| e.id.clone());
        assert_eq!(got, expected);
        assert!(load_entities_by_id(&pool, &[])
            .await
            .expect("empty")
            .is_empty());

        let ideas = all.iter().find(|e| e.id == ideas.id).expect("ideas");
        assert_eq!(ideas.links.len(), 3);
        assert_eq!(ideas.referenced_dates, ["2026-01-02", "2026-09-01"]);
    }

    // I-48: entities written in the same millisecond come back in id order.
    #[tokio::test]
    async fn list_ties_are_broken_by_id() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let mut ids = Vec::new();
        for name in ["C", "A", "B", "D"] {
            ids.push(create(&svc, note(name)).await.id);
        }
        let newest = create(&svc, tag("Newest")).await;
        sqlx::query("UPDATE entities SET updated_at = 1000 WHERE id != ?")
            .bind(&newest.id)
            .execute(&pool)
            .await
            .expect("tie updated_at");
        sqlx::query("UPDATE entities SET updated_at = 2000 WHERE id = ?")
            .bind(&newest.id)
            .execute(&pool)
            .await
            .expect("newest");
        ids.sort();

        let listed = |structure_type: &str| {
            svc.list_entities(Request::new(ListEntitiesRequest {
                structure_type: structure_type.to_string(),
            }))
        };
        let all: Vec<String> = listed("")
            .await
            .expect("list")
            .into_inner()
            .entities
            .into_iter()
            .map(|e| e.id)
            .collect();
        let mut expected = vec![newest.id.clone()];
        expected.extend(ids.iter().cloned());
        assert_eq!(all, expected);

        let notes: Vec<String> = listed("Note")
            .await
            .expect("list notes")
            .into_inner()
            .entities
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(notes, ids);

        let by_id: Vec<String> = load_entities_by_id(&pool, &ids)
            .await
            .expect("by id")
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(by_id, ids);
    }

    async fn list_backlinks_of(svc: &EntityService, id: &str) -> Result<Vec<Backlink>, Status> {
        svc.list_backlinks(Request::new(ListBacklinksRequest {
            entity_id: id.to_string(),
        }))
        .await
        .map(|r| r.into_inner().backlinks)
    }

    /// Pin the created_at of `source`'s link rows from `property_id`.
    async fn set_link_created_at(pool: &SqlitePool, source: &str, property_id: &str, millis: i64) {
        sqlx::query(
            "UPDATE links SET created_at = ? WHERE entity_id = ? AND source_property_id = ?",
        )
        .bind(millis)
        .bind(source)
        .bind(property_id)
        .execute(pool)
        .await
        .expect("set link created_at");
    }

    /// (source id, source property id, created_at millis) of each backlink, in order.
    fn backlink_rows(backlinks: &[Backlink]) -> Vec<(String, String, i64)> {
        backlinks
            .iter()
            .map(|b| {
                (
                    b.source.as_ref().expect("source").id.clone(),
                    b.source_property_id.clone(),
                    millis(b.created_at),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn list_backlinks_excludes_self_links() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let target = create(&svc, note("Target")).await;
        let source = create(&svc, note("Source")).await;
        put_content(&pool, &target.id, &[mention("mention", &target)]).await;
        put_content(&pool, &source.id, &[mention("mention", &target)]).await;

        let backlinks = list_backlinks_of(&svc, &target.id)
            .await
            .expect("backlinks");

        let ids: Vec<&str> = backlinks
            .iter()
            .map(|b| b.source.as_ref().expect("source").id.as_str())
            .collect();
        assert_eq!(ids, [source.id.as_str()]);
    }

    #[tokio::test]
    async fn list_backlinks_reports_property_and_time_newest_first() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let target = create(&svc, note("Target")).await;
        let source = create(&svc, note("Source")).await;
        put_content(&pool, &source.id, &[mention("mention", &target)]).await;
        let chore = create(
            &svc,
            with_relations(todo("Chore"), "related", &[(&target.id, "Note")]),
        )
        .await;
        // The chore also mentions the target in its content: a second row.
        put_content(&pool, &chore.id, &[mention("mention", &target)]).await;
        set_link_created_at(&pool, &source.id, "content", 1_000).await;
        set_link_created_at(&pool, &chore.id, "related", 2_000).await;
        set_link_created_at(&pool, &chore.id, "content", 3_000).await;

        let backlinks = list_backlinks_of(&svc, &target.id)
            .await
            .expect("backlinks");

        assert_eq!(
            backlink_rows(&backlinks),
            [
                (chore.id.clone(), "content".to_string(), 3_000),
                (chore.id.clone(), "related".to_string(), 2_000),
                (source.id.clone(), "content".to_string(), 1_000),
            ]
        );
        // Each source is the full entity, as Get returns it.
        let loaded_source = load_entity(&pool, &source.id).await.expect("load source");
        assert_eq!(backlinks[2].source.as_ref(), Some(&loaded_source));

        // Equal times: the lower source id first.
        set_link_created_at(&pool, &source.id, "content", 5_000).await;
        set_link_created_at(&pool, &chore.id, "content", 5_000).await;
        set_link_created_at(&pool, &chore.id, "related", 5_000).await;
        let backlinks = list_backlinks_of(&svc, &target.id)
            .await
            .expect("backlinks");
        let first = backlink_rows(&backlinks)[0].0.clone();
        assert_eq!(first, source.id.clone().min(chore.id.clone()));
    }

    #[tokio::test]
    async fn list_backlinks_of_missing_entity_is_not_found() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = list_backlinks_of(&svc, "missing")
            .await
            .expect_err("unknown entity");

        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    type EventStream = BoxStream<'static, Result<WatchEntitiesResponse, Status>>;

    /// Open a Watch stream on `svc` through the RPC.
    async fn open_watch(svc: &EntityService) -> EventStream {
        svc.watch_entities(Request::new(WatchEntitiesRequest {}))
            .await
            .expect("watch")
            .into_inner()
    }

    /// The stream's next message, failing the test if none comes within a second.
    async fn next_event(stream: &mut EventStream) -> WatchEntitiesResponse {
        tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("an event within a second")
            .expect("stream still open")
            .expect("event, not an error")
    }

    /// The sorted entity ids of a snapshot event.
    fn snapshot_ids(event: &WatchEntitiesResponse) -> Vec<String> {
        let Some(watch_entities_response::Event::Snapshot(snapshot)) = &event.event else {
            panic!("expected a snapshot, got {event:?}");
        };
        let mut ids: Vec<String> = snapshot.entities.iter().map(|e| e.id.clone()).collect();
        ids.sort();
        ids
    }

    /// The id of an upserted event.
    fn upserted_id(event: &WatchEntitiesResponse) -> &str {
        let Some(watch_entities_response::Event::Upserted(entity)) = &event.event else {
            panic!("expected an upsert, got {event:?}");
        };
        &entity.id
    }

    fn sorted(mut ids: Vec<String>) -> Vec<String> {
        ids.sort();
        ids
    }

    #[tokio::test]
    async fn watch_starts_with_a_snapshot_of_existing_entities() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, tag("b")).await;

        // No writes follow: the snapshot must come without waiting for an event.
        let mut stream = open_watch(&svc).await;
        let first = next_event(&mut stream).await;

        assert_eq!(
            snapshot_ids(&first),
            sorted(vec![a.id.clone(), b.id.clone()])
        );
        assert_eq!(first.revision, 2);
        assert_eq!(first.revision, svc.hub.current_revision());
        let Some(watch_entities_response::Event::Snapshot(snapshot)) = first.event else {
            unreachable!()
        };
        let snapshot_a = snapshot.entities.iter().find(|e| e.id == a.id);
        assert_eq!(snapshot_a, Some(&a));
    }

    #[tokio::test]
    async fn watch_events_after_snapshot_have_increasing_revisions() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let mut stream = open_watch(&svc).await;
        let snapshot = next_event(&mut stream).await;
        assert!(snapshot_ids(&snapshot).is_empty());

        let a = create(&svc, note("A")).await;
        let b = create(&svc, todo("B")).await;
        rename(&svc, &a.id, "A2").await.expect("rename");
        svc.delete_entity(Request::new(DeleteEntityRequest { id: b.id.clone() }))
            .await
            .expect("delete");

        let mut revisions = vec![snapshot.revision];
        let mut kinds = vec![];
        for _ in 0..4 {
            let event = next_event(&mut stream).await;
            revisions.push(event.revision);
            kinds.push(match event.event {
                Some(watch_entities_response::Event::Upserted(e)) => format!("upserted {}", e.name),
                Some(watch_entities_response::Event::DeletedId(id)) => format!("deleted {id}"),
                other => panic!("unexpected event {other:?}"),
            });
        }
        assert_eq!(
            kinds,
            [
                "upserted A".to_string(),
                "upserted B".to_string(),
                "upserted A2".to_string(),
                format!("deleted {}", b.id),
            ]
        );
        assert!(
            revisions.windows(2).all(|w| w[0] < w[1]),
            "revisions increase: {revisions:?}"
        );
    }

    #[tokio::test]
    async fn watch_does_not_repeat_events_covered_by_the_snapshot() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        // Subscribe, then publish before the stream takes its snapshot: the
        // window between `watch_entities`'s subscribe and its revision read.
        let rx = svc.hub.subscribe();
        let a = create(&svc, note("A")).await;
        let mut stream = watch_stream(pool.clone(), svc.hub.clone(), rx);

        let snapshot = next_event(&mut stream).await;
        assert_eq!(snapshot_ids(&snapshot), vec![a.id.clone()]);
        assert_eq!(snapshot.revision, 1);

        // A's upsert is still queued on the receiver; the stream must skip it.
        let b = create(&svc, note("B")).await;
        let next = next_event(&mut stream).await;
        assert_eq!(upserted_id(&next), b.id);
        assert_eq!(next.revision, 2);
    }

    #[tokio::test]
    async fn watch_resyncs_with_a_snapshot_after_lag() {
        let pool = memory_pool().await;
        let svc = EntityService::new(pool, WatchHub::with_capacity(2), EmbedHandle::disabled());
        let mut stream = open_watch(&svc).await;
        assert!(snapshot_ids(&next_event(&mut stream).await).is_empty());

        // Five events into a channel of two, without reading: the stream lags.
        let mut ids = vec![];
        for i in 0..5 {
            ids.push(create(&svc, note(&format!("N{i}"))).await.id);
        }

        let resync = next_event(&mut stream).await;
        assert_eq!(snapshot_ids(&resync), sorted(ids));
        assert_eq!(resync.revision, 5);

        // The two events still in the channel are covered by the resync: the
        // stream reads and skips them, sending nothing.
        let quiet = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next()).await;
        assert!(quiet.is_err(), "expected no event, got {quiet:?}");

        // The next event is the next write.
        let after = create(&svc, note("After")).await;
        let next = next_event(&mut stream).await;
        assert_eq!(upserted_id(&next), after.id);
        assert_eq!(next.revision, 6);
    }

    async fn attach(svc: &EntityService, entity_id: &str, facet: &str) -> Result<Entity, Status> {
        svc.attach_facet(Request::new(AttachFacetRequest {
            entity_id: entity_id.to_string(),
            facet: facet.to_string(),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    async fn detach(svc: &EntityService, entity_id: &str, facet: &str) -> Result<Entity, Status> {
        svc.detach_facet(Request::new(DetachFacetRequest {
            entity_id: entity_id.to_string(),
            facet: facet.to_string(),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    async fn facet_row_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM entity_facets")
            .fetch_one(pool)
            .await
            .expect("count entity_facets")
    }

    /// The one `upserted` entity on `rx`, and nothing after it.
    fn only_upsert(rx: &mut broadcast::Receiver<WatchEntitiesResponse>) -> Entity {
        let event = rx.try_recv().expect("an event");
        let Some(watch_entities_response::Event::Upserted(entity)) = event.event else {
            panic!("expected an upsert, got {event:?}");
        };
        assert!(rx.try_recv().is_err(), "no further events");
        entity
    }

    #[tokio::test]
    async fn create_with_a_facet_attaches_it_with_defaults() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let created = create(&svc, with_facets(note("Ship"), &["todo"])).await;

        assert_eq!(created.facets, vec!["todo".to_string()]);
        assert_eq!(
            select_value(&created, "todo.status").as_deref(),
            Some("open")
        );
        assert_eq!(
            select_value(&created, "todo.priority").as_deref(),
            Some("none")
        );
        assert!(!created.properties.contains_key("todo.due"));
        assert_eq!(facet_row_count(&pool).await, 1);
        assert_eq!(svc.load_entity(&created.id).await.expect("load"), created);
    }

    #[tokio::test]
    async fn create_with_a_facet_lets_the_caller_set_its_values() {
        let svc = entity_service(memory_pool().await);

        let req = with_select(
            with_facets(note("Ship"), &["todo"]),
            "todo.priority",
            "high",
        );
        let created = create(&svc, req).await;

        assert_eq!(
            select_value(&created, "todo.status").as_deref(),
            Some("open")
        );
        assert_eq!(
            select_value(&created, "todo.priority").as_deref(),
            Some("high")
        );
    }

    #[tokio::test]
    async fn create_with_a_value_for_an_unrequested_facet_is_failed_precondition() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = try_create(&svc, with_select(note("Ship"), "todo.status", "done"))
            .await
            .expect_err("todo isn't requested");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(
            err.message().contains("attach the todo facet"),
            "{}",
            err.message()
        );
        assert_eq!(entity_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn create_with_an_unknown_facet_is_invalid_argument() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = try_create(&svc, with_facets(note("Ship"), &["chore"]))
            .await
            .expect_err("unknown facet");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("chore"), "{}", err.message());
        assert_eq!(entity_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn create_with_a_facet_the_structure_does_not_accept_is_failed_precondition() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());

        let err = try_create(&svc, with_facets(tag("urgent"), &["todo"]))
            .await
            .expect_err("a Tag doesn't accept todo");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(entity_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn attach_writes_select_defaults_and_publishes() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let ship = create(&svc, note("Ship")).await;
        let mut rx = svc.hub.subscribe();
        // So the bump shows: timestamps are in milliseconds.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let attached = attach(&svc, &ship.id, "todo").await.expect("attach");

        assert_eq!(attached.facets, vec!["todo".to_string()]);
        assert_eq!(
            select_value(&attached, "todo.status").as_deref(),
            Some("open")
        );
        assert_eq!(
            select_value(&attached, "todo.priority").as_deref(),
            Some("none")
        );
        assert!(millis(attached.updated_at) > millis(ship.updated_at));
        assert_eq!(only_upsert(&mut rx), attached);
    }

    #[tokio::test]
    async fn attach_twice_is_a_no_op() {
        let svc = entity_service(memory_pool().await);
        let ship = create(&svc, note("Ship")).await;
        let first = attach(&svc, &ship.id, "todo").await.expect("attach");
        let mut rx = svc.hub.subscribe();

        let second = attach(&svc, &ship.id, "todo").await.expect("attach again");

        assert_eq!(second, first);
        assert_eq!(second.updated_at, first.updated_at);
        assert!(rx.try_recv().is_err(), "no event");
    }

    #[tokio::test]
    async fn attach_checks_the_entity_and_the_facet() {
        let svc = entity_service(memory_pool().await);
        let ship = create(&svc, note("Ship")).await;
        let label = create(&svc, tag("urgent")).await;

        let missing = attach(&svc, "missing", "todo").await.expect_err("missing");
        assert_eq!(missing.code(), tonic::Code::NotFound);
        let unknown = attach(&svc, &ship.id, "chore").await.expect_err("unknown");
        assert_eq!(unknown.code(), tonic::Code::InvalidArgument);
        let refused = attach(&svc, &label.id, "todo").await.expect_err("refused");
        assert_eq!(refused.code(), tonic::Code::FailedPrecondition);
    }

    #[tokio::test]
    async fn detach_deletes_the_facet_values_and_publishes() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
        let ship = set_property(&svc, &ship.id, "todo.due", date("2026-11-01"))
            .await
            .expect("set due");
        let mut rx = svc.hub.subscribe();
        // So the bump shows: timestamps are in milliseconds.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let detached = detach(&svc, &ship.id, "todo").await.expect("detach");

        assert!(detached.facets.is_empty());
        assert!(
            detached
                .properties
                .keys()
                .all(|id| !id.starts_with("todo.")),
            "{:?}",
            detached.properties.keys()
        );
        assert!(stored_property_ids(&pool, &ship.id).await.is_empty());
        assert_eq!(facet_row_count(&pool).await, 0);
        assert!(millis(detached.updated_at) > millis(ship.updated_at));
        assert_eq!(only_upsert(&mut rx), detached);
    }

    #[tokio::test]
    async fn detach_leaves_structure_properties_and_tags() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let urgent = create(&svc, tag("urgent")).await;
        let req = with_relations(
            with_facets(note("Ship"), &["todo"]),
            "tags",
            &[(&urgent.id, "Tag")],
        );
        let ship = create(&svc, req).await;
        // An ad-hoc id that only looks like the namespace, in another case.
        let ship = set_property(
            &svc,
            &ship.id,
            "TODO.note",
            Some(property_value::Value::Text("kept".to_string())),
        )
        .await
        .expect("set ad-hoc");

        let detached = detach(&svc, &ship.id, "todo").await.expect("detach");

        assert_eq!(
            relation_ids(&detached, "tags"),
            Some(vec![urgent.id.clone()])
        );
        assert_eq!(link_targets(&detached, "tags"), link_targets(&ship, "tags"));
        assert_eq!(
            stored_property_ids(&pool, &ship.id).await,
            vec!["TODO.note".to_string(), "tags".to_string()]
        );
    }

    #[tokio::test]
    async fn detach_when_not_attached_is_a_no_op() {
        let svc = entity_service(memory_pool().await);
        let ship = create(&svc, note("Ship")).await;
        let mut rx = svc.hub.subscribe();

        let same = detach(&svc, &ship.id, "todo").await.expect("detach");

        assert_eq!(same, ship);
        assert!(rx.try_recv().is_err(), "no event");
    }

    #[tokio::test]
    async fn detach_checks_the_entity_and_the_facet() {
        let svc = entity_service(memory_pool().await);
        let ship = create(&svc, note("Ship")).await;

        let missing = detach(&svc, "missing", "todo").await.expect_err("missing");
        assert_eq!(missing.code(), tonic::Code::NotFound);
        let unknown = detach(&svc, &ship.id, "chore").await.expect_err("unknown");
        assert_eq!(unknown.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn reattach_after_detach_writes_fresh_defaults() {
        let svc = entity_service(memory_pool().await);
        let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
        set_property(&svc, &ship.id, "todo.status", select("done"))
            .await
            .expect("set status");
        detach(&svc, &ship.id, "todo").await.expect("detach");

        let again = attach(&svc, &ship.id, "todo").await.expect("attach");

        assert_eq!(select_value(&again, "todo.status").as_deref(), Some("open"));
    }

    async fn add_target(
        svc: &EntityService,
        entity_id: &str,
        property_id: &str,
        target_id: &str,
    ) -> Result<Entity, Status> {
        svc.add_relation_target(Request::new(AddRelationTargetRequest {
            entity_id: entity_id.to_string(),
            property_id: property_id.to_string(),
            target_id: target_id.to_string(),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    async fn remove_target(
        svc: &EntityService,
        entity_id: &str,
        property_id: &str,
        target_id: &str,
    ) -> Result<Entity, Status> {
        svc.remove_relation_target(Request::new(RemoveRelationTargetRequest {
            entity_id: entity_id.to_string(),
            property_id: property_id.to_string(),
            target_id: target_id.to_string(),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    /// The (id, structure_type) refs of a `relations` property, in stored order.
    fn relation_refs(entity: &Entity, property_id: &str) -> Vec<(String, String)> {
        match entity
            .properties
            .get(property_id)
            .and_then(|v| v.value.as_ref())
        {
            Some(property_value::Value::Relations(list)) => list
                .refs
                .iter()
                .map(|r| (r.id.clone(), r.structure_type.clone()))
                .collect(),
            None => vec![],
            other => panic!("expected relations, got {other:?}"),
        }
    }

    /// `(entity.id, "Tag")` pairs for `tags`, the shape `relation_refs` and
    /// `link_targets` return.
    fn tag_refs(tags: &[&Entity]) -> Vec<(String, String)> {
        tags.iter()
            .map(|t| (t.id.clone(), "Tag".to_string()))
            .collect()
    }

    #[tokio::test]
    async fn add_relation_target_appends_links_and_publishes() {
        let svc = entity_service(memory_pool().await);
        let a = create(&svc, tag("a")).await;
        let b = create(&svc, tag("b")).await;
        let entity = tagged_todo(&svc, &[&a]).await;
        let mut rx = svc.hub.subscribe();

        let updated = add_target(&svc, &entity.id, "tags", &b.id)
            .await
            .expect("add");

        assert_eq!(relation_refs(&updated, "tags"), tag_refs(&[&a, &b]));
        let mut linked = tag_refs(&[&a, &b]);
        linked.sort();
        assert_eq!(link_targets(&updated, "tags"), linked);
        assert!(millis(updated.updated_at) > millis(entity.updated_at));
        assert_eq!(svc.load_entity(&entity.id).await.expect("load"), updated);
        assert_eq!(only_upsert(&mut rx), updated);
    }

    #[tokio::test]
    async fn add_relation_target_twice_is_a_no_op() {
        let svc = entity_service(memory_pool().await);
        let a = create(&svc, tag("a")).await;
        let entity = tagged_todo(&svc, &[&a]).await;
        let mut rx = svc.hub.subscribe();

        let same = add_target(&svc, &entity.id, "tags", &a.id)
            .await
            .expect("add again");

        assert_eq!(same, entity);
        assert_eq!(millis(same.updated_at), millis(entity.updated_at));
        assert!(rx.try_recv().is_err(), "no event");
    }

    // I-34's done-when at the RPC level: two writers add different targets to the
    // same starting list, and neither add undoes the other.
    #[tokio::test]
    async fn two_adds_of_different_targets_both_survive() {
        let svc = entity_service(memory_pool().await);
        let seed = create(&svc, tag("seed")).await;
        let a = create(&svc, tag("a")).await;
        let b = create(&svc, tag("b")).await;
        let entity = tagged_todo(&svc, &[&seed]).await;

        add_target(&svc, &entity.id, "tags", &a.id)
            .await
            .expect("add a");
        add_target(&svc, &entity.id, "tags", &b.id)
            .await
            .expect("add b");

        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(relation_refs(&stored, "tags"), tag_refs(&[&seed, &a, &b]));
        assert_eq!(link_targets(&stored, "tags").len(), 3);
    }

    #[tokio::test]
    async fn add_relation_target_checks_entity_property_and_target() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, tag("a")).await;
        let other = create(&svc, note("Not a tag")).await;
        let entity = create(&svc, note("Ship")).await;

        let missing = add_target(&svc, "missing", "tags", &a.id)
            .await
            .expect_err("unknown entity");
        assert_eq!(missing.code(), tonic::Code::NotFound);

        for (entity_id, property_id, target_id) in [
            ("", "tags", a.id.as_str()),
            (entity.id.as_str(), "", a.id.as_str()),
            (entity.id.as_str(), "tags", ""),
        ] {
            let err = add_target(&svc, entity_id, property_id, target_id)
                .await
                .expect_err("empty id");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }

        // Undeclared, or declared but not a `relations` property.
        for property_id in ["foo", "content"] {
            let err = add_target(&svc, &entity.id, property_id, &a.id)
                .await
                .expect_err("not a relations property");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
            assert!(
                err.message().contains("is not a relations property"),
                "{}",
                err.message()
            );
        }
        let with_todo = create(&svc, with_facets(note("Todo"), &["todo"])).await;
        let status = add_target(&svc, &with_todo.id, "todo.status", &a.id)
            .await
            .expect_err("a select");
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        let unknown = add_target(&svc, &entity.id, "tags", "missing")
            .await
            .expect_err("unknown target");
        assert_eq!(unknown.code(), tonic::Code::NotFound);

        let wrong = add_target(&svc, &entity.id, "tags", &other.id)
            .await
            .expect_err("a Note isn't a Tag");
        assert_eq!(wrong.code(), tonic::Code::InvalidArgument);
        assert!(
            wrong.message().contains("must reference a Tag"),
            "{}",
            wrong.message()
        );

        assert!(stored_property_ids(&pool, &entity.id).await.is_empty());
        assert_eq!(svc.load_entity(&entity.id).await.expect("load"), entity);
    }

    // ADR 12, rule 9: no write may name a property id starting with `@`, since
    // those are the link scopes the server derives.
    #[tokio::test]
    async fn reserved_property_ids_are_invalid_argument() {
        use crate::proto::rich_text_service_server::RichTextService as _;
        use crate::proto::GetRichTextRequest;

        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let rich = richtext_service(pool.clone(), WatchHub::new());
        let entity = create(&svc, note("Ship")).await;
        let target = create(&svc, note("Dock")).await;
        let before = entity_count(&pool).await;

        fn assert_reserved(err: Status) {
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
            assert!(err.message().contains("reserved"), "{}", err.message());
        }

        let create_err = try_create(
            &svc,
            with_value(
                note("Sneaky"),
                "@x",
                property_value::Value::Text("v".into()),
            ),
        )
        .await
        .expect_err("CreateEntity with @x");
        assert_reserved(create_err);
        assert_eq!(entity_count(&pool).await, before);

        let text = Some(property_value::Value::Text("v".into()));
        assert_reserved(
            set_property(&svc, &entity.id, "@related", text)
                .await
                .expect_err("SetEntityProperty with @related"),
        );
        assert_reserved(
            set_property(&svc, &entity.id, "@related", None)
                .await
                .expect_err("clearing @related"),
        );
        assert_reserved(
            add_target(&svc, &entity.id, "@related", &target.id)
                .await
                .expect_err("AddRelationTarget with @related"),
        );
        assert_reserved(
            remove_target(&svc, &entity.id, "@related", &target.id)
                .await
                .expect_err("RemoveRelationTarget with @related"),
        );

        assert_reserved(
            rich.put_rich_text(Request::new(PutRichTextRequest {
                entity_id: entity.id.clone(),
                property_id: "@x".to_string(),
                doc: r#"{"type":"doc","content":[]}"#.to_string(),
                expected_updated_at: None,
            }))
            .await
            .expect_err("PutRichText with @x"),
        );
        assert_reserved(
            rich.get_rich_text(Request::new(GetRichTextRequest {
                entity_id: entity.id.clone(),
                property_id: "@x".to_string(),
            }))
            .await
            .expect_err("GetRichText with @x"),
        );

        assert!(stored_property_ids(&pool, &entity.id).await.is_empty());
        assert_eq!(svc.load_entity(&entity.id).await.expect("load"), entity);
    }

    #[tokio::test]
    async fn relation_target_on_unattached_facet_property_is_failed_precondition() {
        let svc = entity_service(memory_pool().await);
        let a = create(&svc, tag("a")).await;
        let ship = create(&svc, note("Ship")).await;
        let mut rx = svc.hub.subscribe();

        let add = add_target(&svc, &ship.id, "todo.status", &a.id)
            .await
            .expect_err("todo isn't attached");
        let remove = remove_target(&svc, &ship.id, "todo.status", &a.id)
            .await
            .expect_err("todo isn't attached");

        for err in [add, remove] {
            assert_eq!(err.code(), tonic::Code::FailedPrecondition);
            assert!(
                err.message()
                    .contains("attach the todo facet before editing todo.status"),
                "{}",
                err.message()
            );
        }
        assert!(rx.try_recv().is_err(), "no event");
    }

    #[tokio::test]
    async fn remove_relation_target_drops_the_ref_and_its_link() {
        let svc = entity_service(memory_pool().await);
        let a = create(&svc, tag("a")).await;
        let b = create(&svc, tag("b")).await;
        let entity = tagged_todo(&svc, &[&a, &b]).await;
        let mut rx = svc.hub.subscribe();

        let updated = remove_target(&svc, &entity.id, "tags", &a.id)
            .await
            .expect("remove");

        assert_eq!(relation_refs(&updated, "tags"), tag_refs(&[&b]));
        assert_eq!(link_targets(&updated, "tags"), tag_refs(&[&b]));
        assert!(millis(updated.updated_at) > millis(entity.updated_at));
        assert_eq!(svc.load_entity(&entity.id).await.expect("load"), updated);
        assert_eq!(only_upsert(&mut rx), updated);
    }

    #[tokio::test]
    async fn remove_last_relation_target_clears_the_property() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, tag("a")).await;
        let entity = tagged_todo(&svc, &[&a]).await;

        let updated = remove_target(&svc, &entity.id, "tags", &a.id)
            .await
            .expect("remove");

        assert!(!updated.properties.contains_key("tags"));
        assert!(link_targets(&updated, "tags").is_empty());
        assert!(!stored_property_ids(&pool, &entity.id)
            .await
            .contains(&"tags".to_string()));
    }

    #[tokio::test]
    async fn remove_relation_target_absent_is_a_no_op() {
        let svc = entity_service(memory_pool().await);
        let a = create(&svc, tag("a")).await;
        let b = create(&svc, tag("b")).await;
        let entity = tagged_todo(&svc, &[&a]).await;
        let untagged = create(&svc, note("Untagged")).await;
        let mut rx = svc.hub.subscribe();

        // A real tag the note doesn't have, an id that doesn't exist, and a
        // note with no `tags` value at all.
        let missing = "missing".to_string();
        for (before, target_id) in [(&entity, &b.id), (&entity, &missing), (&untagged, &a.id)] {
            let same = remove_target(&svc, &before.id, "tags", target_id)
                .await
                .expect("remove absent");
            assert_eq!(&same, before);
        }

        assert!(rx.try_recv().is_err(), "no event");
    }

    async fn add_rel(
        svc: &EntityService,
        entity_id: &str,
        target_id: &str,
    ) -> Result<Entity, Status> {
        svc.add_relationship(Request::new(AddRelationshipRequest {
            entity_id: entity_id.to_string(),
            target_id: target_id.to_string(),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    async fn remove_rel(
        svc: &EntityService,
        entity_id: &str,
        target_id: &str,
    ) -> Result<Entity, Status> {
        svc.remove_relationship(Request::new(RemoveRelationshipRequest {
            entity_id: entity_id.to_string(),
            target_id: target_id.to_string(),
        }))
        .await
        .map(|r| r.into_inner().entity.expect("entity"))
    }

    async fn relationship_row_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM relationships")
            .fetch_one(pool)
            .await
            .expect("count relationships")
    }

    /// Insert a relationship row directly, bypassing `relates_to`.
    async fn insert_relationship_row(pool: &SqlitePool, entity_id: &str, target_id: &str) {
        sqlx::query(
            "INSERT INTO relationships (entity_id, target_id, created_at) VALUES (?, ?, 1)",
        )
        .bind(entity_id)
        .bind(target_id)
        .execute(pool)
        .await
        .expect("insert relationship row");
    }

    #[tokio::test]
    async fn add_relationship_writes_the_row_and_a_related_link() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, note("B")).await;
        let mut rx = svc.hub.subscribe();

        let updated = add_rel(&svc, &a.id, &b.id).await.expect("add");

        assert_eq!(
            link_targets(&updated, RELATED_SCOPE),
            [(b.id.clone(), "Note".to_string())]
        );
        assert_eq!(relationship_row_count(&pool).await, 1);
        assert!(millis(updated.updated_at) > millis(a.updated_at));
        assert_eq!(svc.load_entity(&a.id).await.expect("load"), updated);
        assert_eq!(only_upsert(&mut rx), updated);

        let backlinks = list_backlinks_of(&svc, &b.id).await.expect("backlinks");
        assert_eq!(
            backlink_rows(&backlinks)
                .into_iter()
                .map(|(id, scope, _)| (id, scope))
                .collect::<Vec<_>>(),
            [(a.id.clone(), "@related".to_string())]
        );
    }

    #[tokio::test]
    async fn add_relationship_twice_is_a_no_op() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, note("B")).await;
        let related = add_rel(&svc, &a.id, &b.id).await.expect("add");
        let mut rx = svc.hub.subscribe();

        let same = add_rel(&svc, &a.id, &b.id).await.expect("add again");

        assert_eq!(same, related);
        assert_eq!(millis(same.updated_at), millis(related.updated_at));
        assert_eq!(relationship_row_count(&pool).await, 1);
        assert!(rx.try_recv().is_err(), "no event");
    }

    #[tokio::test]
    async fn add_relationship_checks_in_order() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, note("B")).await;
        let urgent = create(&svc, tag("urgent")).await;
        let mut rx = svc.hub.subscribe();

        let unknown_source = add_rel(&svc, "missing", &b.id).await.expect_err("source");
        assert_eq!(unknown_source.code(), tonic::Code::NotFound);

        // A Tag can't hold relationships, and that check comes before the target's.
        let tag_unknown_target = add_rel(&svc, &urgent.id, "missing")
            .await
            .expect_err("tag, unknown target");
        assert_eq!(tag_unknown_target.code(), tonic::Code::FailedPrecondition);
        let tag_source = add_rel(&svc, &urgent.id, &a.id).await.expect_err("tag");
        assert_eq!(tag_source.code(), tonic::Code::FailedPrecondition);

        let unknown_target = add_rel(&svc, &a.id, "missing").await.expect_err("target");
        assert_eq!(unknown_target.code(), tonic::Code::NotFound);

        let to_self = add_rel(&svc, &a.id, &a.id).await.expect_err("self");
        assert_eq!(to_self.code(), tonic::Code::InvalidArgument);

        let to_tag = add_rel(&svc, &a.id, &urgent.id)
            .await
            .expect_err("Note->Tag");
        assert_eq!(to_tag.code(), tonic::Code::InvalidArgument);

        for (entity_id, target_id) in [("", b.id.as_str()), (a.id.as_str(), ""), ("", "")] {
            let err = add_rel(&svc, entity_id, target_id)
                .await
                .expect_err("empty id");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }

        // The current registry decides, not what allowed a row when it was written:
        // an existing Note -> Tag row doesn't make the same add a no-op.
        insert_relationship_row(&pool, &a.id, &urgent.id).await;
        let stale = add_rel(&svc, &a.id, &urgent.id).await.expect_err("stale");
        assert_eq!(stale.code(), tonic::Code::InvalidArgument);

        assert_eq!(
            relationship_row_count(&pool).await,
            1,
            "only the direct row"
        );
        assert!(rx.try_recv().is_err(), "no event");
    }

    #[tokio::test]
    async fn remove_relationship_drops_the_row_and_link() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, note("B")).await;
        let c = create(&svc, note("C")).await;
        add_rel(&svc, &a.id, &b.id).await.expect("add b");
        let both = add_rel(&svc, &a.id, &c.id).await.expect("add c");
        let mut rx = svc.hub.subscribe();

        let updated = remove_rel(&svc, &a.id, &b.id).await.expect("remove");

        assert_eq!(
            link_targets(&updated, RELATED_SCOPE),
            [(c.id.clone(), "Note".to_string())]
        );
        assert_eq!(relationship_row_count(&pool).await, 1);
        assert!(millis(updated.updated_at) > millis(both.updated_at));
        assert_eq!(svc.load_entity(&a.id).await.expect("load"), updated);
        assert_eq!(only_upsert(&mut rx), updated);
        let backlinks = list_backlinks_of(&svc, &b.id).await.expect("backlinks");
        assert!(backlinks.is_empty());
    }

    #[tokio::test]
    async fn remove_relationship_absent_is_a_no_op() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, note("B")).await;
        let c = create(&svc, note("C")).await;
        let related = add_rel(&svc, &a.id, &b.id).await.expect("add");
        let mut rx = svc.hub.subscribe();

        // A real note it isn't related to, an id that doesn't exist, and a note with
        // no relationships at all.
        let missing = "missing".to_string();
        for (before, target_id) in [(&related, &c.id), (&related, &missing), (&b, &a.id)] {
            let same = remove_rel(&svc, &before.id, target_id)
                .await
                .expect("remove absent");
            assert_eq!(&same, before);
        }

        assert_eq!(relationship_row_count(&pool).await, 1);
        assert!(rx.try_recv().is_err(), "no event");

        let unknown_source = remove_rel(&svc, "missing", &b.id)
            .await
            .expect_err("source");
        assert_eq!(unknown_source.code(), tonic::Code::NotFound);
        let empty = remove_rel(&svc, &a.id, "").await.expect_err("empty id");
        assert_eq!(empty.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn remove_relationship_ignores_relates_to() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let urgent = create(&svc, tag("urgent")).await;
        let a = create(&svc, note("A")).await;
        // A row a narrowed `relates_to` would leave behind: Tag relates to nothing.
        insert_relationship_row(&pool, &urgent.id, &a.id).await;

        let updated = remove_rel(&svc, &urgent.id, &a.id).await.expect("remove");

        assert!(link_targets(&updated, RELATED_SCOPE).is_empty());
        assert_eq!(relationship_row_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn relationship_writes_keep_content_links() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, note("B")).await;
        let mention_b = mention("mention", &b);
        put_content(&pool, &a.id, std::slice::from_ref(&mention_b)).await;
        let linked = [(b.id.clone(), "Note".to_string())];

        let related = add_rel(&svc, &a.id, &b.id).await.expect("add");
        assert_eq!(link_targets(&related, "content"), linked);
        assert_eq!(link_targets(&related, RELATED_SCOPE), linked);

        // Removing the relationship keeps the mention's link.
        let unrelated = remove_rel(&svc, &a.id, &b.id).await.expect("remove");
        assert_eq!(link_targets(&unrelated, "content"), linked);
        assert!(link_targets(&unrelated, RELATED_SCOPE).is_empty());

        // A rich-text save keeps the relationship's link, even one that drops the mention.
        add_rel(&svc, &a.id, &b.id).await.expect("add again");
        put_content(&pool, &a.id, &[]).await;
        let saved = svc.load_entity(&a.id).await.expect("load");
        assert!(link_targets(&saved, "content").is_empty());
        assert_eq!(link_targets(&saved, RELATED_SCOPE), linked);
    }

    #[tokio::test]
    async fn deleting_a_target_republishes_its_relationship_sources() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let b = create(&svc, note("B")).await;
        // `a` also holds a `related` property value naming B, so `strip_relation_refs`
        // changes it too: it's published once, not twice.
        let a = create(
            &svc,
            with_relations(todo("A"), "related", &[(&b.id, "Note")]),
        )
        .await;
        let c = create(&svc, note("C")).await;
        let other = create(&svc, note("Other")).await;
        add_rel(&svc, &a.id, &b.id).await.expect("a -> b");
        add_rel(&svc, &c.id, &b.id).await.expect("c -> b");
        add_rel(&svc, &c.id, &other.id).await.expect("c -> other");
        let a_before = svc.load_entity(&a.id).await.expect("load a");
        let c_before = svc.load_entity(&c.id).await.expect("load c");
        let mut rx = svc.hub.subscribe();

        delete(&svc, &b.id).await;

        let first = rx.recv().await.expect("deleted event");
        assert_eq!(
            first.event,
            Some(watch_entities_response::Event::DeletedId(b.id.clone()))
        );
        let mut expected = [(&a_before, &a), (&c_before, &c)];
        expected.sort_by_key(|(_, e)| e.id.clone());
        for (before, source) in expected {
            let event = rx.recv().await.expect("upserted event");
            let Some(watch_entities_response::Event::Upserted(upserted)) = event.event else {
                panic!("expected an upsert, got {event:?}");
            };
            assert_eq!(upserted.id, source.id);
            assert!(millis(upserted.updated_at) > millis(before.updated_at));
            assert!(
                !upserted
                    .links
                    .iter()
                    .any(|l| l.target.as_ref().is_some_and(|t| t.id == b.id)),
                "no link to the deleted target is left"
            );
            assert_eq!(upserted, svc.load_entity(&source.id).await.expect("load"));
        }
        assert!(rx.try_recv().is_err(), "no further events");

        // C's relationship to the other note is untouched.
        let c_after = svc.load_entity(&c.id).await.expect("load c");
        assert_eq!(
            link_targets(&c_after, RELATED_SCOPE),
            [(other.id.clone(), "Note".to_string())]
        );
        assert_eq!(relationship_row_count(&pool).await, 1);
    }

    #[tokio::test]
    async fn deleting_a_source_removes_its_relationships() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let a = create(&svc, note("A")).await;
        let b = create(&svc, note("B")).await;
        add_rel(&svc, &a.id, &b.id).await.expect("add");
        let mut rx = svc.hub.subscribe();

        delete(&svc, &a.id).await;

        assert_eq!(relationship_row_count(&pool).await, 0);
        let first = rx.recv().await.expect("deleted event");
        assert_eq!(
            first.event,
            Some(watch_entities_response::Event::DeletedId(a.id.clone()))
        );
        assert!(rx.try_recv().is_err(), "the target isn't republished");
        let b_after = svc.load_entity(&b.id).await.expect("load b");
        assert_eq!(b_after, b);
    }

    #[tokio::test]
    async fn set_facet_property_without_the_facet_is_failed_precondition() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let ship = create(&svc, note("Ship")).await;

        let err = set_property(&svc, &ship.id, "todo.status", select("done"))
            .await
            .expect_err("todo isn't attached");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(
            err.message()
                .contains("attach the todo facet before setting todo.status"),
            "{}",
            err.message()
        );
        assert!(stored_property_ids(&pool, &ship.id).await.is_empty());
    }

    #[tokio::test]
    async fn set_facet_property_checks_its_select_options() {
        let svc = entity_service(memory_pool().await);
        let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;

        let err = set_property(&svc, &ship.id, "todo.status", select("someday"))
            .await
            .expect_err("not an option");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("open, done"), "{}", err.message());

        let wrong_kind = set_property(&svc, &ship.id, "todo.due", select("open"))
            .await
            .expect_err("due is a date");
        assert_eq!(wrong_kind.code(), tonic::Code::InvalidArgument);

        let saved = set_property(&svc, &ship.id, "todo.status", select("done"))
            .await
            .expect("set status");
        assert_eq!(select_value(&saved, "todo.status").as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn clearing_a_facet_property_without_the_facet_is_allowed() {
        let svc = entity_service(memory_pool().await);
        let ship = create(&svc, note("Ship")).await;

        let saved = set_property(&svc, &ship.id, "todo.status", None)
            .await
            .expect("clear");

        assert!(saved.properties.is_empty());
        assert!(saved.facets.is_empty());
    }

    #[tokio::test]
    async fn delete_removes_facet_rows() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
        let _kept = create(&svc, with_facets(note("Kept"), &["todo"])).await;
        assert_eq!(facet_row_count(&pool).await, 2);

        delete(&svc, &ship.id).await;

        assert_eq!(facet_row_count(&pool).await, 1);
    }

    #[test]
    fn facets_load_in_registry_order_with_unknown_ones_last() {
        let mut facets = vec!["zeta".to_string(), "todo".to_string(), "alpha".to_string()];

        sort_facets(&mut facets);

        assert_eq!(facets, vec!["todo", "alpha", "zeta"]);
    }
}
