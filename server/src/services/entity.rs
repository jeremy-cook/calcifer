use futures::StreamExt;
use sqlx::SqlitePool;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

use crate::embed::EmbedHandle;
use crate::error::AppError;
use crate::link_store::sync_relation_links;
use crate::proto::{
    entity_event, entity_service_server::EntityService as EntityServiceTrait, property_value,
    CreateDailyNoteRequest, CreateEntityRequest, DeleteEntityRequest, Entity, EntityEvent,
    EntityRef, GetEntityRequest, LinkRef, ListEntitiesRequest, ListEntitiesResponse, Property,
    PropertyValue, RichTextRef, ResolveByNameRequest, ResolveByNameResponse, UpdateEntityRequest,
    WatchRequest,
};
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

    /// Replace an entity's properties wholesale inside a transaction.
    async fn replace_properties(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        entity: &Entity,
    ) -> Result<(), AppError> {
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
    if entity.structure_type != "DailyNote" {
        return None;
    }
    entity.properties.iter().find_map(|p| match &p.value {
        Some(PropertyValue {
            value: Some(property_value::Value::Date(d)),
        }) if p.id == "date" && !d.is_empty() => Some(d.clone()),
        _ => None,
    })
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

/// Build a new Entity for ResolveByName's create path, carrying its structure's
/// richtext property pointers and select defaults (mirrors the FE's buildEntityMessage).
fn build_resolved_entity(structure_type: &str, name: &str) -> Entity {
    let id = uuid::Uuid::new_v4().to_string();
    let mut properties: Vec<Property> = crate::structures::richtext_properties(structure_type)
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
    properties.extend(crate::structures::select_defaults(structure_type).iter().map(
        |(pid, default)| Property {
            id: pid.to_string(),
            value: Some(PropertyValue {
                value: Some(property_value::Value::Select(default.to_string())),
            }),
        },
    ));
    Entity {
        id,
        structure_type: structure_type.to_string(),
        name: name.to_string(),
        properties,
        links: vec![],
        referenced_dates: vec![],
        created_at: None,
        updated_at: None,
    }
}

/// Build a new DailyNote entity for `date`: a `date` property (drives date_key),
/// the structure's richtext `content` property, and a long-human-date name.
fn build_daily_note(date: &str) -> Entity {
    let id = uuid::Uuid::new_v4().to_string();
    let mut properties: Vec<Property> = crate::structures::richtext_properties("DailyNote")
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
    properties.push(Property {
        id: "date".to_string(),
        value: Some(PropertyValue {
            value: Some(property_value::Value::Date(date.to_string())),
        }),
    });
    Entity {
        id,
        structure_type: "DailyNote".to_string(),
        name: format_long_date(date),
        properties,
        links: vec![],
        referenced_dates: vec![],
        created_at: None,
        updated_at: None,
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
        let entity = req
            .into_inner()
            .entity
            .ok_or_else(|| Status::invalid_argument("missing entity"))?;

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
        let entity = req
            .into_inner()
            .entity
            .ok_or_else(|| Status::invalid_argument("missing entity"))?;
        let now = chrono::Utc::now().timestamp_millis();
        let date_key = date_key_for(&entity);

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let updated = sqlx::query!(
            "UPDATE entities SET structure_type = ?, name = ?, date_key = ?, updated_at = ? WHERE id = ?",
            entity.structure_type,
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

        // Create: a fresh entity carrying its structure's richtext properties.
        let entity = build_resolved_entity(&r.structure_type, name);
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

        let entity = build_daily_note(date);
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
    use crate::test_support::{entity_service, memory_pool, note};

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
}
