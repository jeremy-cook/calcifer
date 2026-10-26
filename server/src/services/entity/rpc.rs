use std::collections::HashMap;

use futures::stream::BoxStream;
use tonic::{Request, Response, Status};

use super::construct::{daily_note_name, new_entity};
use super::fts::{fts_delete, fts_upsert_name};
use super::keys::{
    declared_keys, key_value, map_unique_violation, property_key_value, set_key, shown_key_value,
};
use super::load::{load_entities, load_entities_by_id, ts_from_millis};
use super::lookup::{accepted_facet, known_facet, known_structure, structure_and_facets};
use super::relations::{strip_relation_refs, RelationTargetOp, RelationshipOp};
use super::validate::{check_name_chars, reject_reserved_property_id, validate_property};
use super::watch::watch_stream;
use super::EntityService;
use crate::db::begin_write;
use crate::error::AppError;
use crate::links::relations::{is_relation_value, sync_relation_property};
use crate::structures::{self, FacetDef, KeySource};
use calcifer::v1::{
    entity_service_server::EntityService as EntityServiceTrait, property_value,
    resolve_entity_request, watch_entities_response, AddRelationTargetRequest,
    AddRelationTargetResponse, AddRelationshipRequest, AddRelationshipResponse, AttachFacetRequest,
    AttachFacetResponse, Backlink, CreateEntityRequest, CreateEntityResponse, DeleteEntityRequest,
    DeleteEntityResponse, DetachFacetRequest, DetachFacetResponse, Entity, GetEntityRequest,
    GetEntityResponse, ListBacklinksRequest, ListBacklinksResponse, ListEntitiesRequest,
    ListEntitiesResponse, PropertyValue, RemoveRelationTargetRequest, RemoveRelationTargetResponse,
    RemoveRelationshipRequest, RemoveRelationshipResponse, RenameEntityRequest,
    RenameEntityResponse, ResolveEntityRequest, ResolveEntityResponse, SetEntityPropertyRequest,
    SetEntityPropertyResponse, WatchEntitiesRequest, WatchEntitiesResponse,
};

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
        if let Some(name) = &name {
            check_name_chars(name)?;
        }
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
    // The name is trimmed, as Create trims it, and can't be blank or contain `|` or `]]`.
    async fn rename_entity(
        &self,
        req: Request<RenameEntityRequest>,
    ) -> Result<Response<RenameEntityResponse>, Status> {
        let RenameEntityRequest { id, name } = req.into_inner();
        let name = name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name can't be blank"));
        }
        check_name_chars(name)?;
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

        // Only this property's links. The old value counts too, so clearing a
        // relation value stored before undeclared properties were refused (I-79)
        // drops its links. Richtext-derived links live under their own
        // source_property_id and aren't touched.
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

        // Self-links are left out, matching the frontend's relationships section.
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
