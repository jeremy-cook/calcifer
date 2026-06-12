use sqlx::SqlitePool;
use tonic::{Request, Response, Status};

use crate::error::AppError;
use crate::links::extract_doc_references;
use crate::proto::{
    rich_text_service_server::RichTextService as RichTextServiceTrait, RichText, RichTextRef,
};
use crate::services::entity::ts_from_millis;

pub struct RichTextService {
    pool: SqlitePool,
}

impl RichTextService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[tonic::async_trait]
impl RichTextServiceTrait for RichTextService {
    async fn get(&self, req: Request<RichTextRef>) -> Result<Response<RichText>, Status> {
        let r = req.into_inner();

        let row = sqlx::query!(
            r#"SELECT doc AS "doc!", updated_at AS "updated_at!"
               FROM richtext WHERE entity_id = ? AND property_id = ?"#,
            r.entity_id,
            r.property_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound(format!("richtext {}/{}", r.entity_id, r.property_id)))?;

        Ok(Response::new(RichText {
            r#ref: Some(RichTextRef {
                entity_id: r.entity_id,
                property_id: r.property_id,
            }),
            doc: row.doc,
            updated_at: Some(ts_from_millis(row.updated_at)),
        }))
    }

    /// The single write path for richtext-sourced graph edges (app-plan Part 6 amendment).
    /// Upserts the doc, then derives THIS property's links and the entity's referenced_dates
    /// from the doc content — so no client (browser, MCP agent, …) ever authors a link directly.
    async fn put(&self, req: Request<RichText>) -> Result<Response<RichText>, Status> {
        let body = req.into_inner();
        let r = body
            .r#ref
            .clone()
            .ok_or_else(|| Status::invalid_argument("richtext missing ref"))?;
        let entity_id = r.entity_id;
        let property_id = r.property_id;
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        // 1. Upsert the doc.
        sqlx::query!(
            "INSERT INTO richtext (entity_id, property_id, doc, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(entity_id, property_id)
             DO UPDATE SET doc = excluded.doc, updated_at = excluded.updated_at",
            entity_id,
            property_id,
            body.doc,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // 2. Derive references from the saved doc.
        let refs = extract_doc_references(&body.doc).map_err(Status::from)?;

        // 3. Scoped link replace for THIS property, preserving link_id/created_at per target.
        let existing = sqlx::query!(
            "SELECT link_id, target_id, created_at FROM links
             WHERE entity_id = ? AND source_property_id = ?",
            entity_id,
            property_id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::from)?;
        let prev: std::collections::HashMap<String, (String, i64)> = existing
            .into_iter()
            .map(|row| (row.target_id, (row.link_id, row.created_at)))
            .collect();

        sqlx::query!(
            "DELETE FROM links WHERE entity_id = ? AND source_property_id = ?",
            entity_id,
            property_id
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        for m in &refs.entities {
            // Drop mentions whose target has no entities row. Safe because the server is
            // authoritative — unlike the FE, where such a chip is a tombstone, not a deletion.
            let exists = sqlx::query_scalar!(r#"SELECT 1 AS "x!" FROM entities WHERE id = ?"#, m.id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?
                .is_some();
            if !exists {
                continue;
            }

            let (link_id, created_at) = match prev.get(&m.id) {
                Some((lid, ts)) => (lid.clone(), *ts),
                None => (uuid::Uuid::new_v4().to_string(), now),
            };
            sqlx::query!(
                "INSERT INTO links (entity_id, link_id, target_id, target_structure, source_property_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
                entity_id,
                link_id,
                m.id,
                m.structure_type,
                property_id,
                created_at,
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }

        // 4. Recompute referenced_dates as the entity-scoped union across all richtext docs.
        let all_docs = sqlx::query_scalar!(
            r#"SELECT doc AS "doc!" FROM richtext WHERE entity_id = ?"#,
            entity_id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::from)?;

        let mut date_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for d in &all_docs {
            if let Ok(dr) = extract_doc_references(d) {
                for iso in dr.dates {
                    date_set.insert(iso);
                }
            }
        }

        sqlx::query!("DELETE FROM referenced_dates WHERE entity_id = ?", entity_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        for iso in &date_set {
            sqlx::query!(
                "INSERT INTO referenced_dates (entity_id, iso_date) VALUES (?, ?)",
                entity_id,
                iso
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }

        tx.commit().await.map_err(AppError::from)?;

        Ok(Response::new(RichText {
            r#ref: body.r#ref,
            doc: body.doc,
            updated_at: Some(ts_from_millis(now)),
        }))
    }
}
