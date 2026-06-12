use futures::StreamExt;
use sqlx::SqlitePool;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

use crate::error::AppError;
use crate::proto::{
    entity_event, entity_service_server::EntityService as EntityServiceTrait, CreateEntityRequest,
    DeleteEntityRequest, Entity, EntityEvent, EntityRef, GetEntityRequest, LinkRef,
    ListEntitiesRequest, ListEntitiesResponse, Property, PropertyValue, UpdateEntityRequest,
    WatchRequest,
};
use crate::watch::WatchHub;

pub struct EntityService {
    pool: SqlitePool,
    hub: WatchHub,
}

impl EntityService {
    pub fn new(pool: SqlitePool, hub: WatchHub) -> Self {
        Self { pool, hub }
    }

    /// Hydrate a full Entity (metadata + properties + links + referenced_dates).
    /// Shared by Get / Create / Update / List so the read shape is defined once.
    async fn load_entity(&self, id: &str) -> Result<Entity, AppError> {
        let row = sqlx::query!(
            r#"SELECT id AS "id!", structure_type AS "structure_type!", name AS "name!",
                      created_at AS "created_at!", updated_at AS "updated_at!"
               FROM entities WHERE id = ?"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("entity {}", id)))?;

        let prop_rows = sqlx::query!(
            r#"SELECT property_id AS "property_id!", value_blob AS "value_blob!"
               FROM properties WHERE entity_id = ?"#,
            id
        )
        .fetch_all(&self.pool)
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
        .fetch_all(&self.pool)
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
        .fetch_all(&self.pool)
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
    /// Relation-property links (none exist yet) will get per-property scoped
    /// replacement when that property type lands.
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

/// Convert unix-millis to a protobuf Timestamp.
pub(crate) fn ts_from_millis(millis: i64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: millis / 1000,
        nanos: ((millis % 1000) * 1_000_000) as i32,
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
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

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

        self.replace_properties(&mut tx, &entity)
            .await
            .map_err(Status::from)?;
        self.replace_links(&mut tx, &entity.id, &entity.links)
            .await
            .map_err(Status::from)?;
        self.replace_referenced_dates(&mut tx, &entity.id, &entity.referenced_dates)
            .await
            .map_err(Status::from)?;

        tx.commit().await.map_err(AppError::from)?;

        let saved = self.load_entity(&entity.id).await.map_err(Status::from)?;
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

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        sqlx::query!(
            "UPDATE entities SET structure_type = ?, name = ?, updated_at = ? WHERE id = ?",
            entity.structure_type,
            entity.name,
            now,
            entity.id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        self.replace_properties(&mut tx, &entity)
            .await
            .map_err(Status::from)?;
        // links / referenced_dates intentionally untouched here — see replace_links docs.

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

        tx.commit().await.map_err(AppError::from)?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::DeletedId(id)),
        });
        Ok(Response::new(()))
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
