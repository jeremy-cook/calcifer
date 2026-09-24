use sqlx::SqlitePool;
use tonic::{Request, Response, Status};

use crate::embed::EmbedHandle;
use crate::error::AppError;
use crate::link_store::replace_scoped_links;
use crate::links::{extract_doc_references, extract_plain_text};
use crate::proto::{
    rich_text_service_server::RichTextService as RichTextServiceTrait, RichText, RichTextRef,
};
use crate::services::entity::ts_from_millis;

pub struct RichTextService {
    pool: SqlitePool,
    embed: EmbedHandle,
}

impl RichTextService {
    pub fn new(pool: SqlitePool, embed: EmbedHandle) -> Self {
        Self { pool, embed }
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
        replace_scoped_links(&mut tx, &entity_id, &property_id, &refs.entities, now)
            .await
            .map_err(Status::from)?;

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

        // 5. Sync the FTS `body` as the entity-scoped concatenation of all its docs'
        //    plain text (mirrors the referenced_dates union above). Preserve `name`,
        //    which is owned by EntityService; delete-then-insert keeps one row per entity.
        let mut fts_body = String::new();
        for d in &all_docs {
            if let Ok(text) = extract_plain_text(d) {
                if text.is_empty() {
                    continue;
                }
                if !fts_body.is_empty() {
                    fts_body.push(' ');
                }
                fts_body.push_str(&text);
            }
        }

        // Runtime (non-macro) queries: sqlx's compile-time introspection chokes on
        // FTS5 virtual tables, so all `entity_fts` access is unchecked.
        let name: String = sqlx::query_scalar("SELECT name FROM entity_fts WHERE entity_id = ?")
            .bind(&entity_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::from)?
            .unwrap_or_default();

        sqlx::query("DELETE FROM entity_fts WHERE entity_id = ?")
            .bind(&entity_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        sqlx::query("INSERT INTO entity_fts (entity_id, name, body) VALUES (?, ?, ?)")
            .bind(&entity_id)
            .bind(&name)
            .bind(&fts_body)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;

        // Content changed: queue the entity for (re)embedding off the hot-path.
        self.embed.enqueue(&entity_id);

        Ok(Response::new(RichText {
            r#ref: body.r#ref,
            doc: body.doc,
            updated_at: Some(ts_from_millis(now)),
        }))
    }
}
