use futures::StreamExt;
use sqlx::SqlitePool;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

use crate::embed::EmbedHandle;
use crate::error::AppError;
use crate::link_store::{is_relation_value, sync_relation_links, sync_relation_property};
use crate::proto::{
    entity_event, entity_service_server::EntityService as EntityServiceTrait, property_value,
    resolve_entity_request, CreateEntityRequest, DeleteEntityRequest, Entity, EntityEvent,
    EntityRef, GetEntityRequest, LinkRef, ListEntitiesRequest, ListEntitiesResponse, Property,
    PropertyValue, RenameEntityRequest, ResolveEntityRequest, ResolveEntityResponse, RichTextRef,
    SetPropertyRequest, WatchRequest,
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

    /// Insert a brand-new entity built by `new_entity` (row, properties, relation
    /// links and FTS name) in one tx and return the hydrated result. Shared by
    /// Create and Resolve. A new entity has no content, so no content-derived
    /// links or referenced dates: those are `RichTextService.Put`'s (ADR 3).
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
        sync_relation_links(&mut tx, entity, now).await?;
        fts_upsert_name(&mut tx, &entity.id, &entity.name).await?;
        tx.commit().await?;
        // New entity: queue for embedding (no-op until it has content, but keeps
        // the path uniform — Resolve's creates flow here too).
        self.embed.enqueue(&entity.id);
        self.load_entity(&entity.id).await
    }

    /// Case-insensitive lookup of an entity id by (structure_type, name). Only
    /// Tag names are unique, so for other structures several entities can match:
    /// the oldest `created_at` wins and `id` breaks ties (D2), so a lookup always
    /// returns the same one.
    async fn find_by_name(
        &self,
        structure_type: &str,
        name: &str,
    ) -> Result<Option<String>, AppError> {
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

    /// The DailyNote for an ISO day, by its `date_key` mirror.
    async fn find_daily_note(&self, date: &str) -> Result<Option<String>, AppError> {
        let row = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM entities
               WHERE structure_type = 'DailyNote' AND date_key = ?"#,
            date
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
        known_structure(structure_type)?;
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

        let entity = new_entity(structure_type, Some(name), vec![]).map_err(Status::from)?;
        let saved = self
            .persist_new_entity(&entity)
            .await
            .map_err(Status::from)?;
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
        // Canonical form only, so one day can't get a second date_key spelling.
        let date = date.trim();
        let canonical = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map(|d| d.format("%Y-%m-%d").to_string());
        if canonical.as_deref() != Ok(date) {
            return Err(Status::invalid_argument(format!(
                "date must be an ISO calendar day (yyyy-MM-dd), got {date:?}"
            )));
        }

        if let Some(id) = self.find_daily_note(date).await.map_err(Status::from)? {
            let entity = self.load_entity(&id).await.map_err(Status::from)?;
            return Ok(ResolveEntityResponse {
                entity: Some(entity),
                created: false,
            });
        }
        if !create_if_missing {
            return Err(Status::not_found(format!("DailyNote for {date}")));
        }

        let entity =
            new_entity("DailyNote", None, vec![date_property(date)]).map_err(Status::from)?;
        let saved = self
            .persist_new_entity(&entity)
            .await
            .map_err(|e| map_unique_violation(e, &entity.name, Some(date)))?;
        Ok(self.created(saved))
    }

    /// Publish a Resolve-created entity and wrap it in the response.
    fn created(&self, saved: Entity) -> ResolveEntityResponse {
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
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

/// Check every property of an entity against its structure. See
/// `validate_property`.
fn validate_properties(entity: &Entity) -> Result<(), AppError> {
    for prop in &entity.properties {
        validate_property(&entity.structure_type, prop)?;
    }
    Ok(())
}

/// Reject a property value that doesn't fit the structure's declaration: a
/// declared property must hold a value of its declared `PropertyKind` (I-16; a
/// rich-text property only a `richtext` ref), a declared select one of its
/// options, and a `select` value is only allowed on a declared select (I-9).
/// Other undeclared (ad-hoc) properties take any value. Structures missing from
/// the registry (rows written before unknown types were rejected, I-24) have no
/// schema and pass unchecked. Shared by Create and Resolve (via
/// `validate_properties`) and SetProperty.
fn validate_property(structure_type: &str, prop: &Property) -> Result<(), AppError> {
    if structures::structure(structure_type).is_none() {
        return Ok(());
    }
    let value = prop.value.as_ref().and_then(|v| v.value.as_ref());
    let Some(def) = structures::property(structure_type, &prop.id) else {
        return match value {
            Some(property_value::Value::Select(key)) => Err(AppError::Invalid(format!(
                "{structure_type}.{} is not a select property (got select {key:?})",
                prop.id
            ))),
            _ => Ok(()),
        };
    };
    if def.kind == PropertyKind::Select {
        return validate_select(structure_type, def, value);
    }
    let got = value.map(value_kind);
    if got != Some(def.kind) {
        return Err(AppError::Invalid(format!(
            "{structure_type}.{} must be a {} value, got {}",
            prop.id,
            kind_name(def.kind),
            got.map_or("no value", kind_name)
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

/// The `PropertyKind` a value's case belongs to.
fn value_kind(value: &property_value::Value) -> PropertyKind {
    match value {
        property_value::Value::Text(_) => PropertyKind::Text,
        property_value::Value::Number(_) => PropertyKind::Number,
        property_value::Value::Date(_) => PropertyKind::Date,
        property_value::Value::Select(_) => PropertyKind::Select,
        property_value::Value::Relation(_) => PropertyKind::Relation,
        property_value::Value::Richtext(_) => PropertyKind::Richtext,
        property_value::Value::Relations(_) => PropertyKind::Relations,
    }
}

/// A kind's name as its `PropertyValue` case is spelled in the proto.
fn kind_name(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Richtext => "richtext",
        PropertyKind::Text => "text",
        PropertyKind::Number => "number",
        PropertyKind::Date => "date",
        PropertyKind::Select => "select",
        PropertyKind::Relation => "relation",
        PropertyKind::Relations => "relations",
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

/// Map a SQLite UNIQUE-constraint failure to `AlreadyExists` with a message for
/// the index that failed, else fall back to the standard AppError -> Status
/// conversion (I-25). `name` and `date` are the name and DailyNote date the write
/// tried to store. Used by every write that can hit `one_daily_note_per_day` or
/// `one_tag_per_name`.
fn map_unique_violation(err: AppError, name: &str, date: Option<&str>) -> Status {
    let AppError::Db(sqlx::Error::Database(ref db)) = err else {
        return Status::from(err);
    };
    if !db.is_unique_violation() {
        return Status::from(err);
    }
    // SQLite names a failed unique index on plain columns by its column list,
    // not the index name: "UNIQUE constraint failed: entities.date_key" for
    // `one_daily_note_per_day` and "UNIQUE constraint failed: entities.name" for
    // `one_tag_per_name` (its COLLATE NOCASE doesn't change that). Only an index
    // on an expression is reported by name. The tests below pin both texts.
    let message = db.message();
    if message.ends_with("entities.date_key") {
        let date = date.unwrap_or("this date");
        Status::already_exists(format!("a DailyNote for {date} already exists"))
    } else if message.ends_with("entities.name") {
        Status::already_exists(format!("a Tag named \"{name}\" already exists"))
    } else {
        Status::already_exists(message.to_string())
    }
}

/// The DailyNote name rule (ADR 8): a DailyNote is named for its date, as a long
/// human date, on every write. `date_key` is the entity's `date_key` mirror (see
/// `date_key_for` and `date_key_from`), so this is None for other structures and
/// for an undated DailyNote, which keep the name they have.
fn daily_note_name(date_key: Option<&str>) -> Option<String> {
    date_key.map(format_long_date)
}

/// Apply `daily_note_name` to a whole entity, for `new_entity` (and so Create and
/// Resolve). SetProperty applies it to the one row.
fn apply_daily_note_name(entity: &mut Entity) {
    if let Some(name) = daily_note_name(date_key_for(entity).as_deref()) {
        entity.name = name;
    }
}

/// Build a new, unsaved entity with every server-owned default (ADR 8): a minted
/// id, a `RichTextRef` for each declared rich-text property and each select's
/// default option, with the caller's `properties` laid over them (the caller wins
/// on the same id). A caller value for a declared rich-text property is rejected:
/// the server owns those refs. A DailyNote with a date is named for it
/// (`apply_daily_note_name`); otherwise the name is `name`, or with none the
/// structure's `default_name`. The caller makes a `unique_names` default free
/// (`EntityService::free_name`); this has no database.
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
        name: name.map_or_else(|| default_name(structure_type), str::to_string),
        properties: defaults,
        links: vec![],
        referenced_dates: vec![],
        created_at: None,
        updated_at: None,
    };
    apply_daily_note_name(&mut entity);
    Ok(entity)
}

/// The name of a new entity created without one: `Untitled <StructureDef.name>`,
/// e.g. "Untitled To-do". Structures missing from the registry use their type.
fn default_name(structure_type: &str) -> String {
    let name = structures::structure(structure_type).map_or(structure_type, |s| s.name);
    format!("Untitled {name}")
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

/// The registry entry for `structure_type`, or `InvalidArgument` for a type the
/// registry doesn't declare (I-24). Every write or lookup that takes a
/// `structure_type` goes through this, so an unknown type can't create an entity.
fn known_structure(structure_type: &str) -> Result<&'static structures::StructureDef, Status> {
    structures::structure(structure_type).ok_or_else(|| {
        Status::invalid_argument(format!("unknown structure_type {structure_type:?}"))
    })
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
        // "" lists every structure.
        if !filter.is_empty() {
            known_structure(&filter)?;
        }

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

    // Create by intent (ADR 8): the server builds the entity with `new_entity`,
    // so a client can't supply its id, links, referenced dates or timestamps.
    // Only `creatable` structures: a DailyNote comes from Resolve's date key.
    async fn create(&self, req: Request<CreateEntityRequest>) -> Result<Response<Entity>, Status> {
        let CreateEntityRequest {
            structure_type,
            name,
            properties,
        } = req.into_inner();
        let def = known_structure(&structure_type)?;
        if !def.creatable {
            return Err(Status::failed_precondition(format!(
                "{structure_type} can't be created with Create; use Resolve to get or create one"
            )));
        }
        let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let mut entity =
            new_entity(&structure_type, name.as_deref(), properties).map_err(Status::from)?;
        if name.is_none() && def.unique_names {
            entity.name = self
                .free_name(&structure_type, &entity.name)
                .await
                .map_err(Status::from)?;
        }

        let saved = self
            .persist_new_entity(&entity)
            .await
            .map_err(|e| {
                map_unique_violation(e, &entity.name, date_key_for(&entity).as_deref())
            })?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
        Ok(Response::new(saved))
    }

    // The name (and its FTS row) and updated_at only: properties are untouched,
    // so a rename doesn't undo a concurrent SetProperty (I-15). A structure with
    // `name_editable = false` (a DailyNote, named for its date) can't be renamed.
    // The name is trimmed, as Create trims it, and can't be blank.
    async fn rename(&self, req: Request<RenameEntityRequest>) -> Result<Response<Entity>, Status> {
        let RenameEntityRequest { id, name } = req.into_inner();
        let name = name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name can't be blank"));
        }
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
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
        .map_err(|e| map_unique_violation(AppError::from(e), name, None))?;
        fts_upsert_name(&mut tx, &id, name)
            .await
            .map_err(Status::from)?;
        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(&id).await.map_err(Status::from)?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(saved.clone())),
        });
        Ok(Response::new(saved))
    }

    // One property row, not the whole entity, so writers editing different
    // properties of the same entity don't undo each other (I-11). Validates the
    // property with the same checks Create uses: `validate_property` (I-9, I-16)
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

        // Clearing is allowed on any property.
        if prop.value.is_some() {
            validate_property(&structure_type, &prop).map_err(Status::from)?;
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
            .map_err(|e| {
                map_unique_violation(
                    AppError::from(e),
                    name.as_deref().unwrap_or_default(),
                    date_key.as_deref(),
                )
            })?;
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

    async fn resolve(
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
    use crate::proto::rich_text_service_server::RichTextService as _;
    use crate::proto::{EntityRefList, RichText};
    use crate::test_support::{entity_service, memory_pool, note, richtext_service, tag, todo};

    async fn create(svc: &EntityService, req: CreateEntityRequest) -> Entity {
        try_create(svc, req).await.expect("create")
    }

    async fn try_create(svc: &EntityService, req: CreateEntityRequest) -> Result<Entity, Status> {
        svc.create(Request::new(req))
            .await
            .map(Response::into_inner)
    }

    /// A Create request with no name, so the server picks the default.
    fn untitled(structure_type: &str) -> CreateEntityRequest {
        CreateEntityRequest {
            structure_type: structure_type.to_string(),
            name: None,
            properties: vec![],
        }
    }

    async fn rename(svc: &EntityService, id: &str, name: &str) -> Result<Entity, Status> {
        svc.rename(Request::new(RenameEntityRequest {
            id: id.to_string(),
            name: name.to_string(),
        }))
        .await
        .map(Response::into_inner)
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

        let first = create(&svc, untitled("Todo")).await;
        let second = create(&svc, untitled("Todo")).await;

        assert!(uuid::Uuid::parse_str(&first.id).is_ok(), "{}", first.id);
        assert_ne!(first.id, second.id);
        assert_eq!(first.structure_type, "Todo");
        // Todo names aren't unique, so the default may repeat.
        assert_eq!(first.name, "Untitled To-do");
        assert_eq!(second.name, "Untitled To-do");
        assert_eq!(select_value(&first, "status").as_deref(), Some("open"));
        assert_eq!(select_value(&first, "priority").as_deref(), Some("none"));
        let content = first
            .properties
            .iter()
            .find(|p| p.id == "content")
            .and_then(|p| p.value.as_ref())
            .and_then(|v| v.value.clone());
        assert_eq!(
            content,
            Some(property_value::Value::Richtext(RichTextRef {
                entity_id: first.id.clone(),
                property_id: "content".to_string(),
            }))
        );
        // Server-owned fields: no links or dates until content is saved (ADR 3).
        assert!(first.links.is_empty());
        assert!(first.referenced_dates.is_empty());
        assert_eq!(millis(first.created_at), millis(first.updated_at));
        assert_eq!(fts_name(&pool, &first.id).await, "Untitled To-do");
        assert_eq!(upserted_ids(&mut rx), [first.id.clone(), second.id.clone()]);

        // A given name is trimmed, and given properties win over the defaults.
        let named = create(
            &svc,
            with_select(todo("  Water plants "), "priority", "high"),
        )
        .await;
        assert_eq!(named.name, "Water plants");
        assert_eq!(select_value(&named, "priority").as_deref(), Some("high"));
        assert_eq!(select_value(&named, "status").as_deref(), Some("open"));

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
                with_select(todo("Before"), "priority", "high"),
                "tags",
                &[(&urgent.id, "Tag")],
            ),
        )
        .await;
        let mut rx = svc.hub.subscribe();

        let renamed = rename(&svc, &entity.id, "After").await.expect("rename");

        assert_eq!(renamed.name, "After");
        // No write can change an entity's type (I-3).
        assert_eq!(renamed.structure_type, "Todo");
        assert_eq!(renamed.properties, entity.properties);
        assert_eq!(renamed.links, entity.links);
        assert_eq!(renamed.created_at, entity.created_at);
        assert!(millis(renamed.updated_at) >= millis(entity.updated_at));
        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored, renamed);
        assert_eq!(fts_name(&pool, &entity.id).await, "After");
        assert_eq!(upserted_ids(&mut rx), [entity.id.clone()]);
    }

    #[tokio::test]
    async fn rename_trims_the_name() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let entity = create(&svc, note("Before")).await;

        let renamed = rename(&svc, &entity.id, "  After \n").await.expect("rename");

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

        set_property(&svc, &snapshot.id, "status", select("done"))
            .await
            .expect("agent sets status");
        rename(&svc, &snapshot.id, "Water the plants")
            .await
            .expect("browser renames");

        let stored = svc.load_entity(&snapshot.id).await.expect("load");
        assert_eq!(stored.name, "Water the plants");
        assert_eq!(select_value(&stored, "status").as_deref(), Some("done"));
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

    /// `req` with `property_id` set to `value`, replacing any earlier value.
    fn with_value(
        mut req: CreateEntityRequest,
        property_id: &str,
        value: property_value::Value,
    ) -> CreateEntityRequest {
        req.properties.retain(|p| p.id != property_id);
        req.properties.push(Property {
            id: property_id.to_string(),
            value: Some(PropertyValue { value: Some(value) }),
        });
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
            with_select(todo("Water plants"), "priority", "urgent"),
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
            "status",
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
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{structure_type:?}");
        }
        let err = resolve(&svc, "Custom", by_name("Anything"), true)
            .await
            .expect_err("resolve of an unknown type");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert_eq!(entity_count(&pool).await, 1);

        let err = svc
            .list(Request::new(ListEntitiesRequest {
                structure_type: "Custom".to_string(),
            }))
            .await
            .expect_err("list of an unknown type");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        // An empty filter still lists every structure.
        let all = svc
            .list(Request::new(ListEntitiesRequest {
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

    // A deleted Tag stays in the Todo's stored `tags` value (Delete only sweeps
    // links), and a tag edit resends the whole list, so a dead ref must not block
    // the write.
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
        svc.delete(Request::new(DeleteEntityRequest {
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

    // I-16: every declared property takes only its declared kind.
    #[tokio::test]
    async fn values_must_match_the_declared_kind() {
        let pool = memory_pool().await;
        let svc = entity_service(pool.clone());
        let target = create(&svc, note("Target")).await;
        let entity = create(&svc, todo("Water plants")).await;

        // A `relations` value on `content` (declared richtext).
        let err = set_property(
            &svc,
            &entity.id,
            "content",
            tags_value(&[(&target.id, "Note")]),
        )
        .await
        .expect_err("relations on a rich-text property");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("richtext"), "{}", err.message());

        // A `date` value on a select, through SetProperty and Create.
        let err = set_property(&svc, &entity.id, "status", date("2026-06-13"))
            .await
            .expect_err("date on a select");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let err = try_create(
            &svc,
            with_value(
                todo("Dated"),
                "priority",
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
            "due",
            Some(property_value::Value::Text("tomorrow".to_string())),
        )
        .await
        .expect_err("text on a date property");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(stored.properties, entity.properties);
        assert_eq!(stored.links, entity.links);
        assert_eq!(entity_count(&pool).await, 2);

        // The matching kinds are accepted: a richtext ref on `content` (until
        // T12 drops the case), a date on `due`, and anything on an undeclared id.
        let content = Some(property_value::Value::Richtext(RichTextRef {
            entity_id: entity.id.clone(),
            property_id: "content".to_string(),
        }));
        set_property(&svc, &entity.id, "content", content)
            .await
            .expect("richtext on content");
        set_property(&svc, &entity.id, "due", date("2026-06-13"))
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
            .put(Request::new(RichText {
                r#ref: Some(RichTextRef {
                    entity_id: entity.id.clone(),
                    property_id: "content".to_string(),
                }),
                doc,
                updated_at: None,
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
                properties: vec![date_property("2026-06-13")],
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

    async fn resolve(
        svc: &EntityService,
        structure_type: &str,
        key: resolve_entity_request::Key,
        create_if_missing: bool,
    ) -> Result<ResolveEntityResponse, Status> {
        svc.resolve(Request::new(ResolveEntityRequest {
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
    fn upserted_ids(rx: &mut tokio::sync::broadcast::Receiver<EntityEvent>) -> Vec<String> {
        let mut ids = vec![];
        while let Ok(event) = rx.try_recv() {
            if let Some(entity_event::Event::Upserted(e)) = event.event {
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
        assert!(entity.properties.iter().any(|p| p.id == "content"));
        assert_eq!(upserted_ids(&mut rx), [entity.id.clone()]);

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
            .resolve(Request::new(ResolveEntityRequest {
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

        let first = resolve(&svc, "Todo", by_name("  Water plants "), true)
            .await
            .expect("create");
        assert!(first.created);
        let entity = first.entity.expect("entity");
        assert_eq!(entity.structure_type, "Todo");
        assert_eq!(entity.name, "Water plants");
        assert_eq!(select_value(&entity, "status").as_deref(), Some("open"));
        assert_eq!(upserted_ids(&mut rx), [entity.id.clone()]);

        // Case-insensitive, and scoped to the structure.
        let again = resolve(&svc, "Todo", by_name("WATER PLANTS"), true)
            .await
            .expect("get");
        assert!(!again.created);
        assert_eq!(again.entity.expect("entity").id, entity.id);
        assert!(upserted_ids(&mut rx).is_empty());

        let note = resolve(&svc, "Note", by_name("Water plants"), true)
            .await
            .expect("a Note of the same name");
        assert!(note.created);
        assert_ne!(note.entity.expect("entity").id, entity.id);
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
}
