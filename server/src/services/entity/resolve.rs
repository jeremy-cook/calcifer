use std::collections::HashMap;

use tonic::{Code, Status};

use super::construct::{date_properties, new_entity};
use super::lookup::known_structure;
use super::validate::check_name;
use super::EntityService;
use crate::dates::is_iso_day;
use crate::error::AppError;
use crate::structures::{self, fold_name, NameRule, StructureDef};
use calcifer::v1::{Entity, ResolveEntityResponse};

/// How many free names `persist_with_free_name` tries before it gives up.
const FREE_NAME_ATTEMPTS: usize = 5;

impl EntityService {
    /// Lookup of an entity id by (structure_type, name), names compared by
    /// `fold_name`. A structure with a name key (Tag) is looked up in
    /// `entity_keys`, so the lookup and the uniqueness rule agree. Other names
    /// aren't unique, so several entities can match: the oldest wins (see
    /// `ids_named`), so a lookup always returns the same one.
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
        Ok(self
            .ids_named(structure_type, name)
            .await?
            .into_iter()
            .next())
    }

    /// The ids of every `structure_type` entity whose name folds as `name` does
    /// (`fold_name`), oldest `created_at` first and `id` breaking ties. More than
    /// one only for a structure without a name key: the MCP tools refuse such a
    /// name rather than pick one (I-82).
    pub async fn ids_named(
        &self,
        structure_type: &str,
        name: &str,
    ) -> Result<Vec<String>, AppError> {
        let key = fold_name(name);
        let ids = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM entities
               WHERE structure_type = ? AND name_key = ?
               ORDER BY created_at, id"#,
            structure_type,
            key
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// `base`, or if a `structure_type` entity already has that name (folded),
    /// the first free `"<base> N"` from N = 2 (`"<base>.N"` under the dotted
    /// name rule, see `NameRule::numbered`). For the
    /// default name of a `unique_names` structure, so a second "+ New Tag"
    /// doesn't clash with the first.
    pub(super) async fn free_name(
        &self,
        structure_type: &str,
        base: &str,
    ) -> Result<String, AppError> {
        let rule = structures::structure(structure_type).map_or(NameRule::Any, |s| s.name_rule);
        let mut candidate = base.to_string();
        let mut n = 1;
        while self
            .find_by_name(structure_type, &candidate)
            .await?
            .is_some()
        {
            n += 1;
            candidate = rule.numbered(base, n);
        }
        Ok(candidate)
    }

    /// Persist an untitled `entity` of a `unique_names` structure under the first
    /// free name from its default (see `free_name`). Another Create can take that
    /// name between the lookup and the insert (I-45), so when the insert fails
    /// because the picked name is now taken, look again.
    pub(super) async fn persist_with_free_name(
        &self,
        mut entity: Entity,
    ) -> Result<Entity, Status> {
        let base = entity.name.clone();
        let mut attempts = 0;
        loop {
            attempts += 1;
            entity.name = self
                .free_name(&entity.structure_type, &base)
                .await
                .map_err(Status::from)?;
            let lost = match self.persist_new_entity(&entity).await {
                Err(lost)
                    if lost.code() == Code::AlreadyExists && attempts < FREE_NAME_ATTEMPTS =>
                {
                    lost
                }
                result => return result,
            };
            // Only a clash on the name it picked is worth another look.
            if self
                .find_by_name(&entity.structure_type, &entity.name)
                .await
                .map_err(Status::from)?
                .is_none()
            {
                return Err(lost);
            }
        }
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

        // Get: a folded name match is the canonical entity.
        if let Some(id) = self
            .find_by_name(structure_type, name)
            .await
            .map_err(Status::from)?
        {
            return self.found(&id).await;
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

        check_name(def, name)?;
        let entity = new_entity(structure_type, Some(name), HashMap::new(), &[]);
        match self.persist_new_entity(&entity).await {
            Ok(saved) => Ok(self.created(saved)),
            Err(lost) if lost.code() == Code::AlreadyExists => {
                let winner = self
                    .find_by_name(structure_type, name)
                    .await
                    .map_err(Status::from)?;
                self.winner_or(winner, lost).await
            }
            Err(e) => Err(e),
        }
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
            return self.found(&id).await;
        }
        if !create_if_missing {
            return Err(Status::not_found(format!("DailyNote for {date}")));
        }

        let entity = new_entity("DailyNote", None, date_properties(date), &[]);
        match self.persist_new_entity(&entity).await {
            Ok(saved) => Ok(self.created(saved)),
            Err(lost) if lost.code() == Code::AlreadyExists => {
                let winner = self
                    .find_by_key("DailyNote", "date", date)
                    .await
                    .map_err(Status::from)?;
                self.winner_or(winner, lost).await
            }
            Err(e) => Err(e),
        }
    }

    /// The response for an entity Resolve found rather than created.
    async fn found(&self, id: &str) -> Result<ResolveEntityResponse, Status> {
        let entity = self.load_entity(id).await.map_err(Status::from)?;
        Ok(ResolveEntityResponse {
            entity: Some(entity),
            created: false,
        })
    }

    /// After a Resolve's create fails its key with `lost` (ALREADY_EXISTS): a
    /// concurrent Resolve created the same key between the lookup and the insert
    /// (I-36). `winner` is the key looked up again; return its entity, or `lost`
    /// if it has been deleted since.
    async fn winner_or(
        &self,
        winner: Option<String>,
        lost: Status,
    ) -> Result<ResolveEntityResponse, Status> {
        match winner {
            Some(id) => self.found(&id).await,
            None => Err(lost),
        }
    }

    /// Publish a Resolve-created entity and wrap it in the response. It has
    /// only defaults, so no links, and no target to publish.
    pub(super) fn created(&self, saved: Entity) -> ResolveEntityResponse {
        self.hub.publish_entity(saved.clone());
        ResolveEntityResponse {
            entity: Some(saved),
            created: true,
        }
    }
}
