use std::collections::HashMap;

use tonic::Status;

use super::construct::{date_properties, new_entity};
use super::lookup::known_structure;
use super::validate::check_name_chars;
use super::EntityService;
use crate::dates::is_iso_day;
use crate::error::AppError;
use crate::proto::{watch_entities_response, Entity, ResolveEntityResponse};
use crate::structures::{self, fold_name, StructureDef};

impl EntityService {
    /// Case-insensitive lookup of an entity id by (structure_type, name). A
    /// structure with a name key (Tag) is looked up in `entity_keys`, folded as
    /// the write path folds it, so the lookup and the uniqueness rule agree.
    /// Other names aren't unique, so several entities can match: the oldest
    /// `created_at` wins and `id` breaks ties, so a lookup always returns the
    /// same one.
    pub(super) async fn find_by_name(
        &self,
        structure_type: &str,
        name: &str,
    ) -> Result<Option<String>, AppError> {
        if let Some(def) = structures::structure(structure_type).and_then(StructureDef::name_key) {
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
    pub(super) async fn free_name(
        &self,
        structure_type: &str,
        base: &str,
    ) -> Result<String, AppError> {
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
    pub(super) async fn find_by_key(
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
    pub(super) async fn resolve_name(
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

        check_name_chars(name)?;
        let entity = new_entity(structure_type, Some(name), HashMap::new(), &[]);
        let saved = self.persist_new_entity(&entity).await?;
        Ok(self.created(saved))
    }

    /// Resolve's `date` key: get the day's DailyNote, or create it (named for the
    /// day by `new_entity`).
    pub(super) async fn resolve_date(
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
    pub(super) fn created(&self, saved: Entity) -> ResolveEntityResponse {
        self.hub
            .publish(watch_entities_response::Event::Upserted(saved.clone()));
        ResolveEntityResponse {
            entity: Some(saved),
            created: true,
        }
    }
}
