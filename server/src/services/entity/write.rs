use tonic::Status;

use super::fts::fts_upsert_name;
use super::keys::{declared_keys, key_value, map_unique_violation, set_key, shown_key_value};
use super::validate::validate_properties;
use super::EntityService;
use crate::db::begin_write;
use crate::error::AppError;
use crate::links::relations::sync_relation_links;
use crate::proto::Entity;

impl EntityService {
    /// Insert a brand-new entity built by `new_entity` (row, properties, relation
    /// links and FTS name) in one tx and return the hydrated result. Shared by
    /// Create and Resolve. A new entity has no content, so no content-derived
    /// links or referenced dates: those are `RichTextService.PutRichText`'s (ADR 3).
    /// Every key the structure declares is written too, so a taken one is
    /// `AlreadyExists` (see `map_unique_violation`), and so is an
    /// `entity_facets` row per attached facet.
    pub(super) async fn persist_new_entity(&self, entity: &Entity) -> Result<Entity, Status> {
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

    /// Replace an entity's properties wholesale inside a transaction, after
    /// checking them against the structure (see `validate_properties`).
    pub(super) async fn replace_properties(
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
}
