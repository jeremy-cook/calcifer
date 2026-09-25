use sqlx::{SqliteConnection, SqlitePool};
use tonic::{Request, Response, Status};

use crate::embed::EmbedHandle;
use crate::error::AppError;
use crate::link_store::replace_scoped_links;
use crate::links::{extract_doc_references, extract_plain_text};
use crate::proto::{
    entity_event, rich_text_service_server::RichTextService as RichTextServiceTrait, EntityEvent,
    RichText, RichTextRef,
};
use crate::services::entity::{load_entity, ts_from_millis};
use crate::structures;
use crate::watch::WatchHub;

pub struct RichTextService {
    pool: SqlitePool,
    hub: WatchHub,
    embed: EmbedHandle,
}

impl RichTextService {
    pub fn new(pool: SqlitePool, hub: WatchHub, embed: EmbedHandle) -> Self {
        Self { pool, hub, embed }
    }
}

/// Check that `r` names a declared rich-text property of an existing entity:
/// `NotFound` if the entity is missing, `InvalidArgument` if its structure
/// declares no such rich-text property.
async fn check_declared(conn: &mut SqliteConnection, r: &RichTextRef) -> Result<(), Status> {
    let structure_type = sqlx::query_scalar!(
        "SELECT structure_type FROM entities WHERE id = ?",
        r.entity_id
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| Status::not_found(format!("entity {}", r.entity_id)))?;

    if !structures::richtext_properties(&structure_type).contains(&r.property_id.as_str()) {
        return Err(Status::invalid_argument(format!(
            "{} is not a rich-text property of {}",
            r.property_id, structure_type
        )));
    }
    Ok(())
}

/// Whether the stored document's `updated_at` (None when nothing is saved)
/// satisfies the Put's `expected_updated_at`. Unset expects nothing; the epoch
/// expects no saved document; anything else must equal the stored value exactly.
fn expectation_met(expected: Option<&prost_types::Timestamp>, stored: Option<i64>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    let epoch = prost_types::Timestamp::default();
    match stored {
        None => *expected == epoch,
        Some(ms) => *expected != epoch && ts_from_millis(ms) == *expected,
    }
}

#[tonic::async_trait]
impl RichTextServiceTrait for RichTextService {
    async fn get(&self, req: Request<RichTextRef>) -> Result<Response<RichText>, Status> {
        let r = req.into_inner();
        let mut conn = self.pool.acquire().await.map_err(AppError::from)?;
        check_declared(&mut conn, &r).await?;

        let row = sqlx::query!(
            r#"SELECT doc AS "doc!", updated_at AS "updated_at!"
               FROM richtext WHERE entity_id = ? AND property_id = ?"#,
            r.entity_id,
            r.property_id
        )
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::from)?;

        // Declared but never saved: an empty doc at the epoch, not an error.
        let (doc, updated_at) = match row {
            Some(row) => (row.doc, row.updated_at),
            None => (String::new(), 0),
        };

        Ok(Response::new(RichText {
            r#ref: Some(r),
            doc,
            updated_at: Some(ts_from_millis(updated_at)),
            expected_updated_at: None,
        }))
    }

    /// The single write path for richtext-sourced graph edges (app-plan Part 6 amendment).
    /// Upserts the doc, then derives THIS property's links and the entity's referenced_dates
    /// from the doc content — so no client (browser, MCP agent, …) ever authors a link directly.
    /// With `expected_updated_at` set, the write is conditional on the stored doc's
    /// `updated_at` (see the proto). Publishes `rich_text_changed` then `upserted`.
    async fn put(&self, req: Request<RichText>) -> Result<Response<RichText>, Status> {
        let body = req.into_inner();
        let r = body
            .r#ref
            .clone()
            .ok_or_else(|| Status::invalid_argument("richtext missing ref"))?;
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        // 0. Validate the target and check the caller's expectation against what's stored.
        check_declared(&mut tx, &r).await?;
        let entity_id = r.entity_id;
        let property_id = r.property_id;

        let previous = sqlx::query_scalar!(
            "SELECT updated_at FROM richtext WHERE entity_id = ? AND property_id = ?",
            entity_id,
            property_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?;

        if !expectation_met(body.expected_updated_at.as_ref(), previous) {
            return Err(Status::failed_precondition(format!(
                "richtext {}/{} changed since it was read",
                entity_id, property_id
            )));
        }

        // Strictly increasing, so two saves in one millisecond never look equal.
        let updated_at = previous.map_or(now, |prev| now.max(prev + 1));

        // 1. Upsert the doc.
        sqlx::query!(
            "INSERT INTO richtext (entity_id, property_id, doc, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(entity_id, property_id)
             DO UPDATE SET doc = excluded.doc, updated_at = excluded.updated_at",
            entity_id,
            property_id,
            body.doc,
            updated_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // Links, referenced_dates and the doc all changed, so the entity did too.
        sqlx::query!(
            "UPDATE entities SET updated_at = ? WHERE id = ?",
            updated_at,
            entity_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // 2. Derive references from the saved doc.
        let refs = extract_doc_references(&body.doc).map_err(Status::from)?;

        // 3. Scoped link replace for THIS property, preserving link_id/created_at per target.
        replace_scoped_links(&mut tx, &entity_id, &property_id, &refs.entities, updated_at)
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

        let saved = RichText {
            r#ref: body.r#ref,
            doc: body.doc,
            updated_at: Some(ts_from_millis(updated_at)),
            expected_updated_at: None,
        };
        let entity = load_entity(&self.pool, &entity_id)
            .await
            .map_err(Status::from)?;
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::RichTextChanged(saved.clone())),
        });
        self.hub.publish(EntityEvent {
            event: Some(entity_event::Event::Upserted(entity)),
        });

        Ok(Response::new(saved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::entity_service_server::EntityService as _;
    use crate::proto::{CreateEntityRequest, Entity};
    use crate::test_support::{entity_service, memory_pool, note, richtext_service, tag};

    const DOC_A: &str =
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a"}]}]}"#;
    const DOC_B: &str =
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"b"}]}]}"#;

    struct Fixture {
        pool: SqlitePool,
        hub: WatchHub,
        svc: RichTextService,
    }

    async fn fixture() -> Fixture {
        let pool = memory_pool().await;
        let hub = WatchHub::new();
        let svc = richtext_service(pool.clone(), hub.clone());
        Fixture { pool, hub, svc }
    }

    async fn create(pool: &SqlitePool, entity: Entity) -> Entity {
        entity_service(pool.clone())
            .create(Request::new(CreateEntityRequest {
                entity: Some(entity),
            }))
            .await
            .expect("create")
            .into_inner()
    }

    fn rt_ref(entity_id: &str, property_id: &str) -> RichTextRef {
        RichTextRef {
            entity_id: entity_id.to_string(),
            property_id: property_id.to_string(),
        }
    }

    async fn put(
        svc: &RichTextService,
        r: RichTextRef,
        doc: &str,
        expected: Option<prost_types::Timestamp>,
    ) -> Result<RichText, Status> {
        svc.put(Request::new(RichText {
            r#ref: Some(r),
            doc: doc.to_string(),
            updated_at: None,
            expected_updated_at: expected,
        }))
        .await
        .map(Response::into_inner)
    }

    async fn get(svc: &RichTextService, r: RichTextRef) -> Result<RichText, Status> {
        svc.get(Request::new(r)).await.map(Response::into_inner)
    }

    #[tokio::test]
    async fn put_with_stale_expected_updated_at_fails() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;
        let r = rt_ref(&entity.id, "content");
        let first = put(&f.svc, r.clone(), DOC_A, None).await.expect("first put");
        put(&f.svc, r.clone(), DOC_B, None).await.expect("second put");

        let err = put(&f.svc, r.clone(), DOC_A, first.updated_at)
            .await
            .expect_err("stale expectation should fail");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(get(&f.svc, r).await.expect("get").doc, DOC_B);
    }

    #[tokio::test]
    async fn put_with_matching_expected_updated_at_succeeds() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;
        let r = rt_ref(&entity.id, "content");
        let first = put(&f.svc, r.clone(), DOC_A, None).await.expect("first put");

        let second = put(&f.svc, r.clone(), DOC_B, first.updated_at)
            .await
            .expect("matching expectation should succeed");

        assert_eq!(second.doc, DOC_B);
        assert_ne!(second.updated_at, first.updated_at);
        assert_eq!(get(&f.svc, r).await.expect("get").doc, DOC_B);
    }

    #[tokio::test]
    async fn put_expecting_nothing_saved_succeeds_on_a_fresh_doc() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;
        let r = rt_ref(&entity.id, "content");

        let saved = put(&f.svc, r, DOC_A, Some(prost_types::Timestamp::default()))
            .await
            .expect("epoch expectation on an unsaved doc should succeed");

        assert_eq!(saved.doc, DOC_A);
    }

    #[tokio::test]
    async fn put_expecting_nothing_saved_fails_when_a_doc_exists() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;
        let r = rt_ref(&entity.id, "content");
        put(&f.svc, r.clone(), DOC_A, None).await.expect("first put");

        let err = put(&f.svc, r.clone(), DOC_B, Some(prost_types::Timestamp::default()))
            .await
            .expect_err("epoch expectation should fail once a doc exists");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(get(&f.svc, r).await.expect("get").doc, DOC_A);
    }

    #[tokio::test]
    async fn put_to_missing_entity_is_not_found() {
        let f = fixture().await;

        let err = put(&f.svc, rt_ref("ghost", "content"), DOC_A, None)
            .await
            .expect_err("put to a missing entity should fail");

        assert_eq!(err.code(), tonic::Code::NotFound);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM richtext")
            .fetch_one(&f.pool)
            .await
            .expect("count richtext");
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn put_to_undeclared_property_is_invalid() {
        let f = fixture().await;
        let n = create(&f.pool, note("N")).await;
        let t = create(&f.pool, tag("T")).await;

        for r in [rt_ref(&n.id, "nope"), rt_ref(&t.id, "content")] {
            let err = put(&f.svc, r, DOC_A, None)
                .await
                .expect_err("put to an undeclared property should fail");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM richtext")
            .fetch_one(&f.pool)
            .await
            .expect("count richtext");
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn put_publishes_rich_text_changed_and_upserted() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;
        let mut rx = f.hub.subscribe();

        let saved = put(&f.svc, rt_ref(&entity.id, "content"), DOC_A, None)
            .await
            .expect("put");

        match rx.try_recv().expect("first event").event {
            Some(entity_event::Event::RichTextChanged(rt)) => assert_eq!(rt, saved),
            other => panic!("expected rich_text_changed first, got {other:?}"),
        }
        match rx.try_recv().expect("second event").event {
            Some(entity_event::Event::Upserted(e)) => {
                assert_eq!(e.id, entity.id);
                assert_eq!(e.updated_at, saved.updated_at);
            }
            other => panic!("expected upserted second, got {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no further events");
    }

    #[tokio::test]
    async fn put_bumps_entity_updated_at() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;

        let saved = put(&f.svc, rt_ref(&entity.id, "content"), DOC_A, None)
            .await
            .expect("put");

        let stored = load_entity(&f.pool, &entity.id).await.expect("load");
        assert_eq!(stored.updated_at, saved.updated_at);
    }

    #[tokio::test]
    async fn consecutive_puts_have_strictly_increasing_updated_at() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;
        let r = rt_ref(&entity.id, "content");

        let mut last: Option<prost_types::Timestamp> = None;
        for doc in [DOC_A, DOC_B, DOC_A, DOC_B] {
            let saved = put(&f.svc, r.clone(), doc, None).await.expect("put");
            let ts = saved.updated_at.expect("updated_at");
            if let Some(prev) = last {
                assert!(
                    (ts.seconds, ts.nanos) > (prev.seconds, prev.nanos),
                    "{ts:?} should be after {prev:?}"
                );
            }
            last = Some(ts);
        }
    }

    #[tokio::test]
    async fn get_unsaved_declared_doc_is_empty() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;

        let rt = get(&f.svc, rt_ref(&entity.id, "content")).await.expect("get");

        assert_eq!(rt.doc, "");
        assert_eq!(rt.updated_at, Some(prost_types::Timestamp::default()));
    }

    #[tokio::test]
    async fn get_validates_entity_and_property() {
        let f = fixture().await;
        let entity = create(&f.pool, note("N")).await;

        let missing = get(&f.svc, rt_ref("ghost", "content")).await.expect_err("missing");
        let undeclared = get(&f.svc, rt_ref(&entity.id, "nope")).await.expect_err("undeclared");

        assert_eq!(missing.code(), tonic::Code::NotFound);
        assert_eq!(undeclared.code(), tonic::Code::InvalidArgument);
    }
}
