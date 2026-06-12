# Phase 8 — Part 6: RichTextService

Part of `app-plan.md` Phase 8. The hot-path service for richtext documents — separate from `EntityService` for the same reason richtext lives in a parallel slice on the FE: list responses stay lean, and editor autosave doesn't ship the entire entity body on every keystroke.

## Goal

Two RPCs, two queries:

- `RichTextService.Get(RichTextRef) → RichText` — load a doc by its `(entity_id, property_id)` key.
- `RichTextService.Put(RichText) → RichText` — upsert a doc.

## Why a separate service

If `Entity` carried its `richtext` doc body on the wire, every editor autosave would round-trip the full metadata too. Worse, `EntityService.List` would either return enormous payloads or have to selectively project — both sins. Splitting the service mirrors the existing FE separation (`store.ts` for entity metadata, `richtext.ts` for doc bodies) and lets us cache/optimize each independently.

## Files this part creates / modifies

```
server/src/services/richtext.rs    ← NEW
server/src/services/mod.rs         ← + pub mod richtext;
server/src/main.rs                 ← MODIFIED (register service)
```

## Steps

### 1. `server/src/services/richtext.rs`

```rust
use sqlx::SqlitePool;
use tonic::{Request, Response, Status};

use crate::error::AppError;
use crate::proto::*;

pub struct RichTextService {
    pool: SqlitePool,
}

impl RichTextService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[tonic::async_trait]
impl rich_text_service_server::RichTextService for RichTextService {
    async fn get(&self, req: Request<RichTextRef>) -> Result<Response<RichText>, Status> {
        let r = req.into_inner();

        let row = sqlx::query!(
            "SELECT doc, updated_at FROM richtext
             WHERE entity_id = ? AND property_id = ?",
            r.entity_id, r.property_id,
        )
        .fetch_optional(&self.pool).await
        .map_err(AppError::from)?;

        let row = row.ok_or_else(|| AppError::NotFound(
            format!("richtext {}/{}", r.entity_id, r.property_id)
        ))?;

        Ok(Response::new(RichText {
            ref_: Some(RichTextRef {
                entity_id: r.entity_id,
                property_id: r.property_id,
            }),
            doc: row.doc,
            updated_at: Some(crate::services::entity::ts_from_millis(row.updated_at)),
        }))
    }

    async fn put(&self, req: Request<RichText>) -> Result<Response<RichText>, Status> {
        let body = req.into_inner();
        let r = body.ref_.as_ref()
            .ok_or_else(|| Status::invalid_argument("richtext missing ref"))?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            "INSERT INTO richtext (entity_id, property_id, doc, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(entity_id, property_id) DO UPDATE SET
               doc = excluded.doc,
               updated_at = excluded.updated_at",
            r.entity_id, r.property_id, body.doc, now,
        )
        .execute(&self.pool).await
        .map_err(AppError::from)?;

        Ok(Response::new(RichText {
            ref_: body.ref_,
            doc: body.doc,
            updated_at: Some(crate::services::entity::ts_from_millis(now)),
        }))
    }
}
```

A note on field naming: protoc generates `ref` as `r#ref` or `ref_` depending on tonic version (since `ref` is a Rust keyword). Adjust to match what `tonic-build` actually generated — check the file under `target/debug/build/calcifer-server-*/out/`.

### 2. SQLite upsert syntax

`ON CONFLICT(entity_id, property_id) DO UPDATE SET ...` is SQLite's standard upsert. `INSERT OR REPLACE` would also work but it deletes-and-reinserts the row (which would re-trigger any future triggers we add). The `ON CONFLICT` form is the more surgical idiom.

### 3. `services/mod.rs`

```rust
pub mod entity;
pub mod richtext;
```

Make `ts_from_millis` `pub` (or move to a small `services/util.rs`) so `richtext.rs` can call it.

### 4. Register the service

`server/src/main.rs`:

```rust
use crate::proto::rich_text_service_server::RichTextServiceServer;
use crate::services::richtext::RichTextService;

Server::builder()
    .add_service(EntityServiceServer::new(EntityService::new(pool.clone())))
    .add_service(RichTextServiceServer::new(RichTextService::new(pool)))
    .serve(addr)
    .await?;
```

The pool is `Clone`-able cheaply (it's an `Arc` internally).

## Verification

1. `cargo build` clean.
2. `grpcurl ... RichTextService/Put` with a body like `{"ref":{"entityId":"e1","propertyId":"content"},"doc":"{\"type\":\"doc\"}"}` succeeds.
3. `sqlite3 server/calcifer.db "SELECT length(doc) FROM richtext;"` returns >0.
4. `grpcurl ... RichTextService/Get` with the same `ref` round-trips the doc.
5. `Put` again with a different `doc` overwrites; `Get` returns the new value.
6. `Get` for a non-existent ref returns `NOT_FOUND`.

## Done when

All 6 verification steps pass; the richtext service runs alongside `EntityService` from the same binary.

## What's next (Part 7)

Wire `Watch` server-streaming, plus `tonic-web` and CORS so the browser can talk to all of this in Part 8.
