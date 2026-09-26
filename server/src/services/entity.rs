use futures::StreamExt;
use sqlx::SqlitePool;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

use crate::embed::EmbedHandle;
use crate::error::AppError;
use crate::link_store::{is_relation_value, sync_relation_links, sync_relation_property};
use crate::proto::{
    entity_event, entity_service_server::EntityService as EntityServiceTrait, property_value,
    CreateDailyNoteRequest, CreateEntityRequest, DeleteEntityRequest, Entity, EntityEvent,
    EntityRef, GetEntityRequest, LinkRef, ListEntitiesRequest, ListEntitiesResponse, Property,
    PropertyValue, RichTextRef, ResolveByNameRequest, ResolveByNameResponse, SetPropertyRequest,
    UpdateEntityRequest, WatchRequest,
};
use crate::structures::{self, PropertyKind};
use crate::watch::WatchHub;

pub struct EntityService {
    pool: SqlitePool,
    hub: WatchHub,
    embed: EmbedHandle,
}

impl EntityService {
    pub fn new(pool: SqlitePool, hub: WatchHub, embed: EmbedHandle) -> Self {
        Self { pool, hub, embed }
    }

    /// Insert a brand-new entity (row + properties + links + dates) in one tx and
    /// return the hydrated result. Shared by Create and ResolveByName.
    async fn persist_new_entity(&self, entity: &Entity) -> Result<Entity, AppError> {
        let now = chrono::Utc::now().timestamp_millis();
        let date_key = date_key_for(entity);
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            "INSERT INTO entities (id, structure_type, name, date_key, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            entity.id,
            entity.structure_type,
            entity.name,
            date_key,
            now,
            now,
        )
        .execute(&mut *tx)
        .await?;
        self.replace_properties(&mut tx, entity).await?;
        self.replace_links(&mut tx, &entity.id, &entity.links).await?;
        self.replace_referenced_dates(&mut tx, &entity.id, &entity.referenced_dates)
            .await?;
        sync_relation_links(&mut tx, entity, now).await?;
        fts_upsert_name(&mut tx, &entity.id, &entity.name).await?;
        tx.commit().await?;
        // New entity: queue for embedding (no-op until it has content, but keeps
        // the path uniform — ResolveByName-create / CreateDailyNote flow here too).
        self.embed.enqueue(&entity.id);
        self.load_entity(&entity.id).await
    }

    /// Case-insensitive lookup of an entity id by (structure_type, name).
    async fn find_by_name(
        &self,
        structure_type: &str,
        name: &str,
    ) -> Result<Option<String>, AppError> {
        let row = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM entities
               WHERE structure_type = ? AND name = ? COLLATE NOCASE
               LIMIT 1"#,
            structure_type,
            name
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Hydrate a full Entity (metadata + properties + links + referenced_dates).
    /// Shared by Get / Create / Update / List so the read shape is defined once.
    async fn load_entity(&self, id: &str) -> Result<Entity, AppError> {
        load_entity(&self.pool, id).await
    }

    /// Replace an entity's properties wholesale inside a transaction, after
    /// checking select values against the structure (see `validate_selects`).
    async fn replace_properties(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        entity: &Entity,
    ) -> Result<(), AppError> {
        validate_selects(entity)?;
        sqlx::query!("DELETE FROM properties WHERE entity_id = ?", entity.id)
            .execute(&mut **tx)
            .await?;

        for prop in &entity.properties {
            let value = prop
                .value
                .as_ref()
                .ok_or_else(|| AppError::Invalid("property missing value".to_string()))?;
            let blob = prost::Message::encode_to_vec(value);
            sqlx::query!(
                "INSERT INTO properties (entity_id, property_id, value_blob) VALUES (?, ?, ?)",
                entity.id,
                prop.id,
                blob,
            )
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Replace an entity's outgoing links wholesale. Used by Create only.
    ///
    /// Update intentionally does NOT call this: richtext-sourced links are owned
    /// by `RichTextService.Put` (scoped by source_property_id, Part 6), so a
    /// metadata-only Update must not wipe an entity's content-derived links.
    /// Relation-property links get their own scoped replacement via
    /// `sync_relation_links`, called separately by both Create and Update.
    async fn replace_links(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        entity_id: &str,
        links: &[LinkRef],
    ) -> Result<(), AppError> {
        sqlx::query!("DELETE FROM links WHERE entity_id = ?", entity_id)
            .execute(&mut **tx)
            .await?;

        for link in links {
            let target = link
                .target
                .as_ref()
                .ok_or_else(|| AppError::Invalid("link missing target".to_string()))?;
            let created_at = link
                .created_at
                .as_ref()
                .map(|t| t.seconds * 1000 + (t.nanos as i64) / 1_000_000)
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

            sqlx::query!(
                "INSERT INTO links (entity_id, link_id, target_id, target_structure, source_property_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
                entity_id,
                link.id,
                target.id,
                target.structure_type,
                link.source_property_id,
                created_at,
            )
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Replace an entity's referenced_dates wholesale. Used by Create only;
    /// the entity-scoped union is recomputed by `Put` (Part 6) thereafter.
    async fn replace_referenced_dates(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        entity_id: &str,
        dates: &[String],
    ) -> Result<(), AppError> {
        sqlx::query!("DELETE FROM referenced_dates WHERE entity_id = ?", entity_id)
            .execute(&mut **tx)
            .await?;

        for iso in dates {
            sqlx::query!(
                "INSERT INTO referenced_dates (entity_id, iso_date) VALUES (?, ?)",
                entity_id,
                iso,
            )
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
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
            Ok::<_, AppError>(Property {
                id: r.property_id,
                value: Some(value),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let link_rows = sqlx::query!(
        r#"SELECT link_id AS "link_id!", target_id AS "target_id!",
                  target_structure AS "target_structure!",
                  source_property_id AS "source_property_id!", created_at AS "created_at!"
           FROM links WHERE entity_id = ?"#,
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
        r#"SELECT iso_date AS "iso_date!" FROM referenced_dates WHERE entity_id = ?"#,
        id
    )
    .fetch_all(pool)
    .await?;

    Ok(Entity {
        id: row.id,
        structure_type: row.structure_type,
        name: row.name,
        properties,
        links,
        referenced_dates,
        created_at: Some(ts_from_millis(row.created_at)),
        updated_at: Some(ts_from_millis(row.updated_at)),
    })
}

/// Sync the FTS `name` column for an entity, preserving the existing `body`
/// (owned by `RichTextService.Put`). Delete-then-insert by entity_id so there is
/// always exactly one `entity_fts` row per entity. Call inside the same tx as the
/// entities-table write.
pub(crate) async fn fts_upsert_name(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    name: &str,
) -> Result<(), AppError> {
    // Runtime (non-macro) queries: sqlx's compile-time introspection chokes on
    // FTS5 virtual tables, so all `entity_fts` access is unchecked.
    let body: String = sqlx::query_scalar("SELECT body FROM entity_fts WHERE entity_id = ?")
        .bind(entity_id)
        .fetch_optional(&mut **tx)
        .await?
        .unwrap_or_default();

    sqlx::query("DELETE FROM entity_fts WHERE entity_id = ?")
        .bind(entity_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO entity_fts (entity_id, name, body) VALUES (?, ?, ?)")
        .bind(entity_id)
        .bind(name)
        .bind(body)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Remove an entity's FTS row. Call inside the same tx as the entity delete.
pub(crate) async fn fts_delete(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM entity_fts WHERE entity_id = ?")
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

/// The `date_key` mirror for an entity: the value of its `date` property when the
/// entity is a DailyNote, else None. Populating this column is what arms the
/// `one_daily_note_per_day` unique index. Returning None for non-DailyNote types
/// keeps the partial index inert for everything else.
fn date_key_for(entity: &Entity) -> Option<String> {
    entity
        .properties
        .iter()
        .find_map(|p| date_key_from(&entity.structure_type, p))
}

/// The `date_key` a single property contributes: its value when it's a
/// DailyNote's non-empty `date`, else None. Shared by `date_key_for` and
/// SetProperty, which only sees the one property.
fn date_key_from(structure_type: &str, prop: &Property) -> Option<String> {
    if structure_type != "DailyNote" || prop.id != "date" {
        return None;
    }
    match &prop.value {
        Some(PropertyValue {
            value: Some(property_value::Value::Date(d)),
        }) if !d.is_empty() => Some(d.clone()),
        _ => None,
    }
}

/// Reject select values the structure doesn't allow (I-9), for every property
/// of an entity. See `validate_select`.
fn validate_selects(entity: &Entity) -> Result<(), AppError> {
    for prop in &entity.properties {
        validate_select(&entity.structure_type, prop)?;
    }
    Ok(())
}

/// Reject a select value the structure doesn't allow (I-9): a property declared as
/// a select must hold a `select` value naming one of its options, and a `select`
/// value is only allowed on a declared select. Structures missing from the
/// registry have no schema and pass unchecked; other kinds aren't checked.
/// Shared by Create/Update (via `validate_selects`) and SetProperty.
fn validate_select(structure_type: &str, prop: &Property) -> Result<(), AppError> {
    if structures::structure(structure_type).is_none() {
        return Ok(());
    }
    let def = structures::property(structure_type, &prop.id)
        .filter(|d| d.kind == PropertyKind::Select);
    let value = prop.value.as_ref().and_then(|v| v.value.as_ref());
    match (def, value) {
        (None, Some(property_value::Value::Select(key))) => Err(AppError::Invalid(format!(
            "{structure_type}.{} is not a select property (got select {key:?})",
            prop.id
        ))),
        (None, _) => Ok(()),
        (Some(def), value) => {
            let allowed: Vec<&str> = def.options.iter().map(|o| o.key).collect();
            let key = match value {
                Some(property_value::Value::Select(key)) => key.as_str(),
                _ => {
                    return Err(AppError::Invalid(format!(
                        "{structure_type}.{} must be a select value, one of: {}",
                        prop.id,
                        allowed.join(", ")
                    )));
                }
            };
            if !allowed.contains(&key) {
                return Err(AppError::Invalid(format!(
                    "invalid value {key:?} for {structure_type}.{}; allowed: {}",
                    prop.id,
                    allowed.join(", ")
                )));
            }
            Ok(())
        }
    }
}

/// Format an ISO calendar day ("2026-06-13") as a long human date
/// ("June 13, 2026"). Falls back to the raw input on a parse failure.
fn format_long_date(date: &str) -> String {
    match chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(d) => d.format("%B %-d, %Y").to_string(),
        Err(_) => date.to_string(),
    }
}

/// Map a SQLite UNIQUE-constraint failure to a tonic status, else fall back to
/// the standard AppError -> Status conversion. Used by the DailyNote create path.
fn map_unique_violation(err: AppError, msg: &str) -> Status {
    if let AppError::Db(sqlx::Error::Database(ref db)) = err {
        if db.is_unique_violation() {
            return Status::already_exists(msg.to_string());
        }
    }
    Status::from(err)
}

/// The DailyNote name rule (ADR 8): a DailyNote is named for its date, as a long
/// human date, on every write. `date_key` is the entity's `date_key` mirror (see
/// `date_key_for` and `date_key_from`), so this is None for other structures and
/// for an undated DailyNote, which keep the name they have.
fn daily_note_name(date_key: Option<&str>) -> Option<String> {
    date_key.map(format_long_date)
}

/// Apply `daily_note_name` to a whole entity, for the write paths that see one
/// (Create, Update and `new_entity`). SetProperty applies it to the one row.
fn apply_daily_note_name(entity: &mut Entity) {
    if let Some(name) = daily_note_name(date_key_for(entity).as_deref()) {
        entity.name = name;
    }
}

/// Build a new, unsaved entity with every server-owned default (ADR 8): a minted
/// id, a `RichTextRef` for each declared rich-text property and each select's
/// default option, with the caller's `properties` laid over them (the caller wins
/// on the same id). A caller value for a declared rich-text property is rejected:
/// the server owns those refs. The name is `name`, or for a DailyNote the one
/// derived from its date (`apply_daily_note_name`).
fn new_entity(
    structure_type: &str,
    name: Option<&str>,
    properties: Vec<Property>,
) -> Result<Entity, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let richtext = structures::richtext_properties(structure_type);
    let mut defaults: Vec<Property> = richtext
        .iter()
        .map(|pid| Property {
            id: pid.to_string(),
            value: Some(PropertyValue {
                value: Some(property_value::Value::Richtext(RichTextRef {
                    entity_id: id.clone(),
                    property_id: pid.to_string(),
                })),
            }),
        })
        .collect();
    defaults.extend(
        structures::select_defaults(structure_type)
            .iter()
            .map(|(pid, default)| Property {
                id: pid.to_string(),
                value: Some(PropertyValue {
                    value: Some(property_value::Value::Select(default.to_string())),
                }),
            }),
    );
    for prop in properties {
        if richtext.contains(&prop.id.as_str()) {
            return Err(AppError::Invalid(format!(
                "{structure_type}.{} is a rich-text property; the server sets its value",
                prop.id
            )));
        }
        match defaults.iter_mut().find(|p| p.id == prop.id) {
            Some(existing) => *existing = prop,
            None => defaults.push(prop),
        }
    }
    let mut entity = Entity {
        id,
        structure_type: structure_type.to_string(),
        name: name.unwrap_or_default().to_string(),
        properties: defaults,
        links: vec![],
        referenced_dates: vec![],
        created_at: None,
        updated_at: None,
    };
    apply_daily_note_name(&mut entity);
    Ok(entity)
}

/// A DailyNote's `date` property holding the ISO day `date`.
fn date_property(date: &str) -> Property {
    Property {
        id: "date".to_string(),
        value: Some(PropertyValue {
            value: Some(property_value::Value::Date(date.to_string())),
        }),
    }
}

#[tonic::async_trait]
impl EntityServiceTrait for EntityService {
    async fn get(&self, req: Request<GetEntityRequest>) -> Result<Response<Entity>, Status> {
        let id = req.into_inner().id;
        self.load_entity(&id)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn list(
        &self,
        req: Request<ListEntitiesRequest>,
    ) -> Result<Response<ListEntitiesResponse>, Status> {
        let filter = req.into_inner().structure_type;

        let ids: Vec<String> = if filter.is_empty() {
            sqlx::query_scalar!(r#"SELECT id AS "id!" FROM entities ORDER BY updated_at DESC"#)
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query_scalar!(
                r#"SELECT id AS "id!" FROM entities WHERE structure_type = ? ORDER BY updated_at DESC"#,
                filter
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(AppError::from)?;

        let mut entities = Vec::with_capacity(ids.len());
        for id in ids {
            entities.push(self.load_entity(&id).await.map_err(Status::from)?);
        }

        Ok(Response::new(ListEntitiesResponse { entities }))
    }

    async fn create(&self, req: Request<CreateEntityRequest>) -> Result<Response<Entity>, Status> {
        let mut entity = req
            .into_inner()
            .entity
            .ok_or_else(|| Status::invalid_argument("missing entity"))?;
        apply_daily_note_name(&mut entity);

        let saved = self
            .persist_new_entity(&entity)
            .await
            .map_err(|e| map_unique_violation(e, "a DailyNote for this date already exists"))?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
        Ok(Response::new(saved))
    }

    async fn update(&self, req: Request<UpdateEntityRequest>) -> Result<Response<Entity>, Status> {
        let mut entity = req
            .into_inner()
            .entity
            .ok_or_else(|| Status::invalid_argument("missing entity"))?;
        apply_daily_note_name(&mut entity);
        let now = chrono::Utc::now().timestamp_millis();
        let date_key = date_key_for(&entity);

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        // An entity's type is fixed at creation: changing it would break property
        // and link assumptions (e.g. a DailyNote without a `date`). Reject rather
        // than silently ignore, so the caller learns its request was wrong.
        let stored_type = sqlx::query_scalar!(
            "SELECT structure_type FROM entities WHERE id = ?",
            entity.id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| Status::not_found(format!("entity {}", entity.id)))?;
        if stored_type != entity.structure_type {
            return Err(Status::invalid_argument(format!(
                "cannot change structure_type of entity {} from {} to {}",
                entity.id, stored_type, entity.structure_type
            )));
        }

        let updated = sqlx::query!(
            "UPDATE entities SET name = ?, date_key = ?, updated_at = ? WHERE id = ?",
            entity.name,
            date_key,
            now,
            entity.id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| map_unique_violation(AppError::from(e), "a DailyNote for this date already exists"))?;
        // Unknown id: bail before the properties insert trips the foreign key.
        if updated.rows_affected() == 0 {
            return Err(Status::not_found(format!("entity {}", entity.id)));
        }

        self.replace_properties(&mut tx, &entity)
            .await
            .map_err(Status::from)?;
        // links / referenced_dates intentionally untouched here — see replace_links docs.

        sync_relation_links(&mut tx, &entity, now)
            .await
            .map_err(Status::from)?;

        fts_upsert_name(&mut tx, &entity.id, &entity.name)
            .await
            .map_err(Status::from)?;

        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(&entity.id).await.map_err(Status::from)?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
        Ok(Response::new(saved))
    }

    // One property row, not the whole entity, so writers editing different
    // properties of the same entity don't undo each other (I-11). Validates the
    // property with the same checks Create/Update use: `validate_select` (I-9)
    // and, through `sync_relation_property`, `check_relation_targets` (I-2).
    // Only a DailyNote's `date` changes the name (and so the FTS name row).
    async fn set_property(
        &self,
        req: Request<SetPropertyRequest>,
    ) -> Result<Response<Entity>, Status> {
        let SetPropertyRequest {
            entity_id,
            property_id,
            value,
        } = req.into_inner();
        if property_id.is_empty() {
            return Err(Status::invalid_argument("property_id is required"));
        }
        // An unset value, or one with no case, clears the property.
        let prop = Property {
            id: property_id,
            value: value.filter(|v| v.value.is_some()),
        };
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let structure_type = sqlx::query_scalar!(
            "SELECT structure_type FROM entities WHERE id = ?",
            entity_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| Status::not_found(format!("entity {}", entity_id)))?;

        // Clearing is allowed on any property, as omitting it from an Update is.
        if prop.value.is_some() {
            validate_select(&structure_type, &prop).map_err(Status::from)?;
        }

        let previous = sqlx::query_scalar!(
            "SELECT value_blob FROM properties WHERE entity_id = ? AND property_id = ?",
            entity_id,
            prop.id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .map(|blob| <PropertyValue as prost::Message>::decode(&*blob))
        .transpose()
        .map_err(AppError::from)?;

        match &prop.value {
            Some(value) => {
                let blob = prost::Message::encode_to_vec(value);
                sqlx::query!(
                    "INSERT INTO properties (entity_id, property_id, value_blob) VALUES (?, ?, ?)
                     ON CONFLICT (entity_id, property_id) DO UPDATE SET value_blob = excluded.value_blob",
                    entity_id,
                    prop.id,
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
                    prop.id
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;
            }
        }

        // Only this property's links. The old value counts too, so replacing or
        // clearing an ad-hoc relation property drops its links. Richtext-derived
        // links live under their own source_property_id and aren't touched.
        let value = prop.value.as_ref().and_then(|v| v.value.as_ref());
        let was_relation = is_relation_value(previous.as_ref().and_then(|v| v.value.as_ref()));
        if structures::relation_properties(&structure_type).contains(&prop.id.as_str())
            || is_relation_value(value)
            || was_relation
        {
            sync_relation_property(&mut tx, &entity_id, &structure_type, &prop.id, value, now)
                .await
                .map_err(Status::from)?;
        }

        // A DailyNote's `date` drives date_key and its name; every other property
        // leaves both be. Clearing the date keeps the name it had.
        if structure_type == "DailyNote" && prop.id == "date" {
            let date_key = date_key_from(&structure_type, &prop);
            let name = daily_note_name(date_key.as_deref());
            sqlx::query!(
                "UPDATE entities SET name = COALESCE(?, name), date_key = ?, updated_at = ? WHERE id = ?",
                name,
                date_key,
                now,
                entity_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| map_unique_violation(AppError::from(e), "a DailyNote for this date already exists"))?;
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
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
        Ok(Response::new(saved))
    }

    async fn delete(&self, req: Request<DeleteEntityRequest>) -> Result<Response<()>, Status> {
        let id = req.into_inner().id;

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        // ON DELETE CASCADE handles this entity's properties / links / referenced_dates.
        sqlx::query!("DELETE FROM entities WHERE id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        // richtext has no FK reference — purge explicitly.
        sqlx::query!("DELETE FROM richtext WHERE entity_id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        // Inbound links authored by OTHER entities aren't covered by the cascade — sweep them
        // so the relational index never points at a tombstone.
        sqlx::query!("DELETE FROM links WHERE target_id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        // entity_fts has no FK reference — purge explicitly (mirrors richtext above).
        fts_delete(&mut tx, &id).await.map_err(Status::from)?;

        // Semantic rows (chunks + their vec0 embeddings) have no FK either — purge
        // explicitly so KNN never returns a tombstone. entity_vec keys on chunk ids.
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
        sqlx::query("DELETE FROM chunks WHERE entity_id = ?")
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::DeletedId(id)),
        });
        Ok(Response::new(()))
    }

    async fn resolve_by_name(
        &self,
        req: Request<ResolveByNameRequest>,
    ) -> Result<Response<ResolveByNameResponse>, Status> {
        let r = req.into_inner();
        let name = r.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }

        // Get: a case-insensitive name match is the canonical entity.
        if let Some(id) = self
            .find_by_name(&r.structure_type, name)
            .await
            .map_err(Status::from)?
        {
            let entity = self.load_entity(&id).await.map_err(Status::from)?;
            return Ok(Response::new(ResolveByNameResponse {
                entity: Some(entity),
                created: false,
            }));
        }

        if !r.create_if_missing {
            return Err(Status::not_found(format!(
                "{} named {:?}",
                r.structure_type, name
            )));
        }

        // Create: a fresh entity carrying its structure's defaults.
        let entity = new_entity(&r.structure_type, Some(name), vec![]).map_err(Status::from)?;
        let saved = self.persist_new_entity(&entity).await.map_err(Status::from)?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
        Ok(Response::new(ResolveByNameResponse {
            entity: Some(saved),
            created: true,
        }))
    }

    async fn list_backlinks(
        &self,
        req: Request<EntityRef>,
    ) -> Result<Response<ListEntitiesResponse>, Status> {
        let target_id = req.into_inner().id;

        let ids: Vec<String> = sqlx::query_scalar!(
            r#"SELECT DISTINCT entity_id AS "entity_id!" FROM links WHERE target_id = ?"#,
            target_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        let mut entities = Vec::with_capacity(ids.len());
        for id in ids {
            entities.push(self.load_entity(&id).await.map_err(Status::from)?);
        }

        Ok(Response::new(ListEntitiesResponse { entities }))
    }

    async fn create_daily_note(
        &self,
        req: Request<CreateDailyNoteRequest>,
    ) -> Result<Response<Entity>, Status> {
        let date = req.into_inner().date;
        let date = date.trim();
        if date.is_empty() {
            return Err(Status::invalid_argument("date is required"));
        }

        let entity = new_entity("DailyNote", None, vec![date_property(date)]).map_err(Status::from)?;
        let saved = self.persist_new_entity(&entity).await.map_err(|e| {
            map_unique_violation(e, &format!("a DailyNote for {} already exists", date))
        })?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
        Ok(Response::new(saved))
    }

    type WatchStream = futures::stream::BoxStream<'static, Result<EntityEvent, Status>>;
    async fn watch(
        &self,
        _: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let rx = self.hub.subscribe();
        let stream = BroadcastStream::new(rx).filter_map(|res| async move {
            match res {
                Ok(event) => Some(Ok(event)),
                Err(_lagged) => None, // slow subscriber: skip lagged events
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::EntityRefList;
    use crate::test_support::{entity_service, memory_pool, note, tag, todo};

    async fn create(svc: &EntityService, entity: Entity) -> Entity {
        svc.create(Request::new(CreateEntityRequest {
            entity: Some(entity),
        }))
        .await
        .expect("create")
        .into_inner()
    }

    #[tokio::test]
    async fn update_renames_existing_entity() {
        let svc = entity_service(memory_pool().await);
        let mut entity = create(&svc, note("Before")).await;

        entity.name = "After".to_string();
        let updated = svc
            .update(Request::new(UpdateEntityRequest {
                entity: Some(entity.clone()),
            }))
            .await
            .expect("update")
            .into_inner();

        assert_eq!(updated.name, "After");
        assert_eq!(updated.structure_type, "Note");
        assert_eq!(updated.properties, entity.properties);
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored.name, "After");
    }

    #[tokio::test]
    async fn update_missing_entity_is_not_found() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        // Carries a property, so without the guard the properties insert would
        // hit the foreign key instead.
        let entity = note("Ghost");

        let err = svc
            .update(Request::new(UpdateEntityRequest {
                entity: Some(entity.clone()),
            }))
            .await
            .expect_err("update of a missing id should fail");

        assert_eq!(err.code(), tonic::Code::NotFound);
        let fts_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity_fts WHERE entity_id = ?")
            .bind(&entity.id)
            .fetch_one(&pool)
            .await
            .expect("count fts");
        assert_eq!(fts_rows, 0);
    }

    #[tokio::test]
    async fn update_rejects_structure_type_change() {
        let svc = entity_service(memory_pool().await);
        let mut entity = create(&svc, note("Typed")).await;

        entity.structure_type = "Tag".to_string();
        entity.name = "Renamed".to_string();
        let err = svc
            .update(Request::new(UpdateEntityRequest {
                entity: Some(entity.clone()),
            }))
            .await
            .expect_err("changing structure_type should be rejected");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored.structure_type, "Note");
        assert_eq!(stored.name, "Typed");
    }

    fn select_value(entity: &Entity, property_id: &str) -> Option<String> {
        entity
            .properties
            .iter()
            .find(|p| p.id == property_id)
            .and_then(|p| p.value.as_ref())
            .and_then(|v| match &v.value {
                Some(property_value::Value::Select(key)) => Some(key.clone()),
                _ => None,
            })
    }

    fn with_select(mut entity: Entity, property_id: &str, key: &str) -> Entity {
        let value = Some(PropertyValue {
            value: Some(property_value::Value::Select(key.to_string())),
        });
        match entity.properties.iter_mut().find(|p| p.id == property_id) {
            Some(p) => p.value = value,
            None => entity.properties.push(Property {
                id: property_id.to_string(),
                value,
            }),
        }
        entity
    }

    #[tokio::test]
    async fn update_rejects_unknown_select_value() {
        let svc = entity_service(memory_pool().await);
        let entity = create(&svc, todo("Water plants")).await;

        let err = svc
            .update(Request::new(UpdateEntityRequest {
                entity: Some(with_select(entity.clone(), "status", "banana")),
            }))
            .await
            .expect_err("unknown status should be rejected");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("banana"), "{}", err.message());
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(select_value(&stored, "status").as_deref(), Some("open"));
    }

    #[tokio::test]
    async fn update_accepts_declared_select_value() {
        let svc = entity_service(memory_pool().await);
        let entity = create(&svc, todo("Water plants")).await;

        let updated = svc
            .update(Request::new(UpdateEntityRequest {
                entity: Some(with_select(entity, "status", "done")),
            }))
            .await
            .expect("update")
            .into_inner();

        assert_eq!(select_value(&updated, "status").as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn create_rejects_unknown_select_value() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let entity = with_select(todo("Water plants"), "priority", "urgent");

        let err = svc
            .create(Request::new(CreateEntityRequest {
                entity: Some(entity.clone()),
            }))
            .await
            .expect_err("unknown priority should be rejected");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities WHERE id = ?")
            .bind(&entity.id)
            .fetch_one(&pool)
            .await
            .expect("count entities");
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn select_values_are_checked_against_the_declared_kind() {
        let svc = entity_service(memory_pool().await);

        // A select on a property the structure doesn't declare as one.
        let err = svc
            .create(Request::new(CreateEntityRequest {
                entity: Some(with_select(note("Stray"), "status", "open")),
            }))
            .await
            .expect_err("select on a non-select property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // A declared select holding some other kind of value.
        let mut entity = todo("Mistyped");
        let status = entity
            .properties
            .iter_mut()
            .find(|p| p.id == "status")
            .expect("status");
        status.value = Some(PropertyValue {
            value: Some(property_value::Value::Text("open".to_string())),
        });
        let err = svc
            .create(Request::new(CreateEntityRequest {
                entity: Some(entity),
            }))
            .await
            .expect_err("text on a select property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // Structures outside the registry have no schema to check against.
        let mut custom = with_select(note("Custom"), "mood", "whatever");
        custom.structure_type = "Custom".to_string();
        svc.create(Request::new(CreateEntityRequest {
            entity: Some(custom),
        }))
        .await
        .expect("unknown structure is unchecked");
    }

    fn with_relations(mut entity: Entity, property_id: &str, refs: &[(&str, &str)]) -> Entity {
        let value = Some(PropertyValue {
            value: Some(property_value::Value::Relations(EntityRefList {
                refs: refs
                    .iter()
                    .map(|(id, structure_type)| EntityRef {
                        id: id.to_string(),
                        structure_type: structure_type.to_string(),
                    })
                    .collect(),
            })),
        });
        entity.properties.retain(|p| p.id != property_id);
        entity.properties.push(Property {
            id: property_id.to_string(),
            value,
        });
        entity
    }

    async fn update(svc: &EntityService, entity: Entity) -> Result<Entity, Status> {
        svc.update(Request::new(UpdateEntityRequest {
            entity: Some(entity),
        }))
        .await
        .map(Response::into_inner)
    }

    /// (target_id, target_structure) of an entity's links from `property_id`.
    fn link_targets(entity: &Entity, property_id: &str) -> Vec<(String, String)> {
        entity
            .links
            .iter()
            .filter(|l| l.source_property_id == property_id)
            .filter_map(|l| l.target.as_ref())
            .map(|t| (t.id.clone(), t.structure_type.clone()))
            .collect()
    }

    #[tokio::test]
    async fn update_rejects_note_in_todo_tags() {
        let svc = entity_service(memory_pool().await);
        let urgent = create(&svc, tag("urgent")).await;
        let other = create(&svc, note("Not a tag")).await;
        let todo = create(
            &svc,
            with_relations(todo("Water plants"), "tags", &[(&urgent.id, "Tag")]),
        )
        .await;

        let err = update(
            &svc,
            with_relations(
                todo.clone(),
                "tags",
                &[(&urgent.id, "Tag"), (&other.id, "Note")],
            ),
        )
        .await
        .expect_err("a Note in tags should be rejected");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("tags"), "{}", err.message());
        let stored = svc.load_entity(&todo.id).await.expect("load");
        assert_eq!(stored.properties, todo.properties);
        assert_eq!(
            link_targets(&stored, "tags"),
            [(urgent.id.clone(), "Tag".to_string())]
        );
    }

    #[tokio::test]
    async fn tag_in_todo_tags_links_with_its_real_type() {
        let svc = entity_service(memory_pool().await);
        let urgent = create(&svc, tag("urgent")).await;
        let todo = create(&svc, todo("Water plants")).await;

        // An empty claimed type is allowed; the link still records the real one.
        let updated = update(&svc, with_relations(todo, "tags", &[(&urgent.id, "")]))
            .await
            .expect("update");

        assert_eq!(
            link_targets(&updated, "tags"),
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
        let err = update(&svc, with_relations(todo, "tags", &[(&target.id, "Tag")]))
            .await
            .expect_err("lying claimed type should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // Ad-hoc properties have no declared target, but the claim is still checked...
        let err = update(
            &svc,
            with_relations(source.clone(), "related", &[(&target.id, "Tag")]),
        )
        .await
        .expect_err("lying claimed type on an ad-hoc property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // ...and a truthful one links with the real type.
        let updated = update(
            &svc,
            with_relations(source, "related", &[(&target.id, "Note")]),
        )
        .await
        .expect("ad-hoc relation to any structure");
        assert_eq!(
            link_targets(&updated, "related"),
            [(target.id.clone(), "Note".to_string())]
        );
    }

    // A deleted Tag stays in the Todo's stored `tags` value (Delete only sweeps
    // links), and every Update resends it, so a dead ref must not block the write.
    #[tokio::test]
    async fn dead_relation_ref_does_not_block_update() {
        let svc = entity_service(memory_pool().await);
        let gone = create(&svc, tag("gone")).await;
        let kept = create(&svc, tag("kept")).await;
        let todo = create(
            &svc,
            with_relations(
                todo("Water plants"),
                "tags",
                &[(&gone.id, "Tag"), (&kept.id, "Tag")],
            ),
        )
        .await;
        svc.delete(Request::new(DeleteEntityRequest {
            id: gone.id.clone(),
        }))
        .await
        .expect("delete tag");
        let todo = svc.load_entity(&todo.id).await.expect("load");

        let updated = update(&svc, with_select(todo, "status", "done"))
            .await
            .expect("status change with a dead tag ref");

        assert_eq!(select_value(&updated, "status").as_deref(), Some("done"));
        assert_eq!(
            link_targets(&updated, "tags"),
            [(kept.id.clone(), "Tag".to_string())]
        );
    }

    async fn set_property(
        svc: &EntityService,
        entity_id: &str,
        property_id: &str,
        value: Option<property_value::Value>,
    ) -> Result<Entity, Status> {
        svc.set_property(Request::new(SetPropertyRequest {
            entity_id: entity_id.to_string(),
            property_id: property_id.to_string(),
            value: value.map(|value| PropertyValue { value: Some(value) }),
        }))
        .await
        .map(Response::into_inner)
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

        set_property(&svc, &snapshot.id, "status", select("done"))
            .await
            .expect("writer A");
        set_property(&svc, &snapshot.id, "priority", select("high"))
            .await
            .expect("writer B");

        let stored = svc.load_entity(&snapshot.id).await.expect("load");
        assert_eq!(select_value(&stored, "status").as_deref(), Some("done"));
        assert_eq!(select_value(&stored, "priority").as_deref(), Some("high"));
    }

    // The contrast: Update resends every property, so a writer holding a stale
    // snapshot silently reverts the other writer's edit. Kept for renames and
    // multi-field edits only.
    #[tokio::test]
    async fn update_from_a_stale_snapshot_loses_a_concurrent_edit() {
        let svc = entity_service(memory_pool().await);
        let snapshot = create(&svc, todo("Water plants")).await;

        set_property(&svc, &snapshot.id, "status", select("done"))
            .await
            .expect("writer A");
        update(&svc, with_select(snapshot.clone(), "priority", "high"))
            .await
            .expect("writer B");

        let stored = svc.load_entity(&snapshot.id).await.expect("load");
        assert_eq!(select_value(&stored, "priority").as_deref(), Some("high"));
        assert_eq!(select_value(&stored, "status").as_deref(), Some("open"));
    }

    #[tokio::test]
    async fn set_property_rejects_unknown_select_value() {
        let svc = entity_service(memory_pool().await);
        let entity = create(&svc, todo("Water plants")).await;

        let err = set_property(&svc, &entity.id, "status", select("banana"))
            .await
            .expect_err("unknown status should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("banana"), "{}", err.message());

        // A select on a property not declared as one is rejected too.
        let err = set_property(&svc, &entity.id, "due", select("open"))
            .await
            .expect_err("select on a non-select property should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored.properties, entity.properties);
        assert_eq!(stored.updated_at, entity.updated_at);
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
        let svc = entity_service(memory_pool().await);
        let urgent = create(&svc, tag("urgent")).await;
        let mentioned = create(&svc, note("Mentioned")).await;
        // Create persists `links` as given, standing in for RichTextService.Put's
        // content-derived links.
        let mut entity = with_relations(todo("Water plants"), "tags", &[(&urgent.id, "Tag")]);
        entity.links.push(LinkRef {
            id: uuid::Uuid::new_v4().to_string(),
            target: Some(EntityRef {
                id: mentioned.id.clone(),
                structure_type: "Note".to_string(),
            }),
            source_property_id: "content".to_string(),
            created_at: None,
        });
        let entity = create(&svc, entity).await;
        assert_eq!(link_targets(&entity, "tags").len(), 1);

        let cleared = set_property(&svc, &entity.id, "tags", None)
            .await
            .expect("clear tags");

        assert!(cleared.properties.iter().all(|p| p.id != "tags"));
        assert!(link_targets(&cleared, "tags").is_empty());
        let mut expected: Vec<_> = entity
            .properties
            .iter()
            .filter(|p| p.id != "tags")
            .cloned()
            .collect();
        let mut remaining = cleared.properties.clone();
        expected.sort_by(|a, b| a.id.cmp(&b.id));
        remaining.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(remaining, expected);
        assert_eq!(
            link_targets(&cleared, "content"),
            [(mentioned.id.clone(), "Note".to_string())]
        );

        // A value with no case clears too.
        set_property(&svc, &entity.id, "status", None)
            .await
            .expect("clear status");
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(select_value(&stored, "status"), None);
        assert_eq!(select_value(&stored, "priority").as_deref(), Some("none"));
    }

    #[tokio::test]
    async fn set_property_missing_entity_is_not_found() {
        let svc = entity_service(memory_pool().await);

        let err = set_property(&svc, "no-such-entity", "status", select("done"))
            .await
            .expect_err("missing entity");

        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    /// A new DailyNote for `date`, built by the server's default builder.
    fn daily_note(date: &str) -> Entity {
        new_entity("DailyNote", None, vec![date_property(date)]).expect("daily note")
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

    async fn stored_date_key(pool: &SqlitePool, entity_id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT date_key FROM entities WHERE id = ?")
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .expect("entity row")
    }

    #[test]
    fn new_entity_builds_defaults_and_lets_the_caller_win() {
        let entity = new_entity(
            "Todo",
            Some("Water plants"),
            vec![Property {
                id: "priority".to_string(),
                value: Some(PropertyValue {
                    value: Some(property_value::Value::Select("high".to_string())),
                }),
            }],
        )
        .expect("todo");

        assert_eq!(entity.name, "Water plants");
        assert_eq!(select_value(&entity, "status").as_deref(), Some("open"));
        assert_eq!(select_value(&entity, "priority").as_deref(), Some("high"));
        assert_eq!(
            entity.properties.iter().filter(|p| p.id == "priority").count(),
            1
        );
        let content = entity
            .properties
            .iter()
            .find(|p| p.id == "content")
            .and_then(|p| p.value.as_ref())
            .and_then(|v| v.value.clone());
        assert_eq!(
            content,
            Some(property_value::Value::Richtext(RichTextRef {
                entity_id: entity.id.clone(),
                property_id: "content".to_string(),
            }))
        );
    }

    #[test]
    fn new_entity_rejects_a_caller_rich_text_value() {
        let err = new_entity(
            "Note",
            Some("Sneaky"),
            vec![Property {
                id: "content".to_string(),
                value: Some(PropertyValue {
                    value: Some(property_value::Value::Richtext(RichTextRef {
                        entity_id: "someone-else".to_string(),
                        property_id: "content".to_string(),
                    })),
                }),
            }],
        )
        .expect_err("rich-text value from the caller");
        assert!(matches!(err, AppError::Invalid(_)), "{err:?}");
    }

    #[test]
    fn new_entity_names_a_daily_note_for_its_date() {
        let entity = new_entity("DailyNote", Some("Ignored"), vec![date_property("2026-06-13")])
            .expect("daily note");
        assert_eq!(entity.name, "June 13, 2026");
    }

    #[tokio::test]
    async fn create_and_update_derive_daily_note_name() {
        let svc = entity_service(memory_pool().await);
        let mut entity = daily_note("2026-06-13");
        entity.name = "Client name".to_string();

        let created = create(&svc, entity).await;
        assert_eq!(created.name, "June 13, 2026");

        let mut moved = with_date(created, "2026-06-14");
        moved.name = "Stale name".to_string();
        let updated = update(&svc, moved).await.expect("update");
        assert_eq!(updated.name, "June 14, 2026");
    }

    fn with_date(mut entity: Entity, d: &str) -> Entity {
        entity.properties.retain(|p| p.id != "date");
        entity.properties.push(date_property(d));
        entity
    }

    #[tokio::test]
    async fn set_property_date_renames_daily_note() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let note = create(&svc, daily_note("2026-06-13")).await;

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
        create(&svc, daily_note("2026-06-13")).await;
        let second = create(&svc, daily_note("2026-06-14")).await;

        let err = set_property(&svc, &second.id, "date", date("2026-06-13"))
            .await
            .expect_err("day already has a note");

        assert_eq!(err.code(), tonic::Code::AlreadyExists);
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
        let first = create(&svc, daily_note("2026-06-13")).await;
        let second = create(&svc, daily_note("2026-06-14")).await;

        set_property(&svc, &first.id, "date", date("2026-06-15"))
            .await
            .expect("move to a free day");
        // The old day is free again; the new one is taken.
        svc.create_daily_note(Request::new(CreateDailyNoteRequest {
            date: "2026-06-13".to_string(),
        }))
        .await
        .expect("old day is free");
        let err = set_property(&svc, &second.id, "date", date("2026-06-15"))
            .await
            .expect_err("day already has a note");
        assert_eq!(err.code(), tonic::Code::AlreadyExists);
    }
}
