# Phase 8 — Part 3: SQLite & first real read

Part of `app-plan.md` Phase 8. Replace the hardcoded `EntityService::Get` with a real DB query. The SQLite schema lands here in its first migration.

> **Amendment (post-0a data-model review).** Two schema refinements land with the migration below:
> 1. **`entities.date_key TEXT`** + a partial unique index moves the "one DailyNote per calendar day" invariant out of app code (`moveDailyNote`) and into the DB:
>    ```sql
>    ALTER TABLE entities ADD COLUMN date_key TEXT;  -- fold into the CREATE TABLE
>    CREATE UNIQUE INDEX one_daily_note_per_day
>      ON entities(date_key)
>      WHERE structure_type = 'DailyNote' AND date_key IS NOT NULL;
>    ```
>    The server mirrors the `date` property's value into `date_key` on write (NULL for non-DailyNotes). A `moveDailyNote` collision then surfaces as a unique-constraint violation → map to `Status::already_exists` (the FE already has refusal UX). Querying "the DailyNote for 2026-06-12" becomes a column lookup instead of decoding every `properties.value_blob`.
> 2. **`links.source_property_id`** (already in the schema) is now meaningfully populated, not always `''` — see the Part 5 / Part 6 amendments for scoped replacement.

## Goal

`Get` returns a row read from `server/calcifer.db`. The compile-time SQL check is the headline — if you typo a column, `cargo build` fails before the binary runs.

## Files this part creates / modifies

```
server/.env                                     ← NEW (DATABASE_URL)
server/migrations/20260509000000_init.sql       ← NEW (full schema)
server/src/db.rs                                ← NEW
server/src/error.rs                             ← NEW
server/src/main.rs                              ← MODIFIED (open pool, pass into service)
server/src/services/entity.rs                   ← MODIFIED (real Get)
server/Cargo.toml                               ← + sqlx, thiserror
```

## Steps

### 1. Add deps

```toml
[dependencies]
# existing...
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "macros", "migrate", "chrono"] }
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
```

`sqlx-cli` for migrations:

```bash
cargo install sqlx-cli --no-default-features --features sqlite
```

### 2. `server/.env`

```
DATABASE_URL=sqlite://./calcifer.db
```

`sqlx`'s `query!` macro reads `DATABASE_URL` **at compile time** to introspect the schema. Without this env var, `cargo build` will fail with a clear error explaining what's missing.

Add `.env` to gitignore (already covered if root gitignore has `*.env` but worth verifying).

### 3. Create the database file

```bash
cd server
sqlx database create   # creates calcifer.db
```

### 4. The first migration

`server/migrations/20260509000000_init.sql`:

```sql
CREATE TABLE entities (
  id              TEXT PRIMARY KEY,
  structure_type  TEXT NOT NULL,
  name            TEXT NOT NULL,
  created_at      INTEGER NOT NULL,    -- unix millis
  updated_at      INTEGER NOT NULL
);

CREATE TABLE properties (
  entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  property_id  TEXT NOT NULL,
  value_blob   BLOB NOT NULL,           -- prost-encoded PropertyValue
  PRIMARY KEY (entity_id, property_id)
);

CREATE TABLE richtext (
  entity_id    TEXT NOT NULL,
  property_id  TEXT NOT NULL,
  doc          TEXT NOT NULL,           -- TipTap JSON
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (entity_id, property_id)
);

CREATE TABLE links (
  entity_id            TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  link_id              TEXT NOT NULL,
  target_id            TEXT NOT NULL,
  target_structure     TEXT NOT NULL,
  source_property_id   TEXT NOT NULL DEFAULT '',
  created_at           INTEGER NOT NULL,
  PRIMARY KEY (entity_id, link_id)
);
CREATE INDEX links_target ON links(target_id);

CREATE TABLE referenced_dates (
  entity_id  TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  iso_date   TEXT NOT NULL,
  PRIMARY KEY (entity_id, iso_date)
);
CREATE INDEX referenced_dates_iso ON referenced_dates(iso_date);
```

Apply it:

```bash
sqlx migrate run
```

### 5. `server/src/db.rs`

```rust
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

pub async fn connect(url: &str) -> sqlx::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;

    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}
```

`sqlx::migrate!()` is a macro that embeds every `.sql` file under `server/migrations/` into the binary at compile time. No runtime migration directory lookup — the migrations ship inside the executable.

### 6. `server/src/error.rs`

```rust
use thiserror::Error;
use tonic::Status;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("decode error")]
    Decode(#[from] prost::DecodeError),
}

impl From<AppError> for Status {
    fn from(err: AppError) -> Self {
        match err {
            AppError::NotFound(msg) => Status::not_found(msg),
            AppError::Db(e) => Status::internal(format!("db: {}", e)),
            AppError::Decode(e) => Status::internal(format!("decode: {}", e)),
        }
    }
}
```

This is the two-tier error pattern: `AppError` is the typed "library" error our service code returns; the `From<AppError> for Status` impl handles the boundary between our domain and the gRPC wire format. RPC handlers return `Result<Response<T>, Status>`, so the `?` operator on `AppError` auto-converts.

### 7. Wire pool into main + service

`server/src/main.rs`:

```rust
mod db;
mod error;
mod proto;
mod services;

use std::net::SocketAddr;
use tonic::transport::Server;

use crate::proto::entity_service_server::EntityServiceServer;
use crate::services::entity::EntityService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = db::connect(&database_url).await?;

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    tracing::info!("listening on {}", addr);

    Server::builder()
        .add_service(EntityServiceServer::new(EntityService::new(pool)))
        .serve(addr)
        .await?;

    Ok(())
}
```

Add `dotenvy = "0.15"` to deps for `.env` loading.

### 8. Real `Get`

`server/src/services/entity.rs`:

```rust
use sqlx::SqlitePool;
use tonic::{Request, Response, Status};

use crate::error::AppError;
use crate::proto::*;

pub struct EntityService {
    pool: SqlitePool,
}

impl EntityService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[tonic::async_trait]
impl entity_service_server::EntityService for EntityService {
    async fn get(&self, req: Request<GetEntityRequest>) -> Result<Response<Entity>, Status> {
        let id = req.into_inner().id;

        let row = sqlx::query!(
            "SELECT id, structure_type, name, created_at, updated_at
             FROM entities WHERE id = ?",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        let row = row.ok_or_else(|| AppError::NotFound(format!("entity {}", id)))?;

        let entity = Entity {
            id: row.id,
            structure_type: row.structure_type,
            name: row.name,
            properties: vec![],          // Part 4
            links: vec![],               // Part 5
            referenced_dates: vec![],    // Part 5
            created_at: Some(prost_types::Timestamp {
                seconds: row.created_at / 1000,
                nanos: ((row.created_at % 1000) * 1_000_000) as i32,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: row.updated_at / 1000,
                nanos: ((row.updated_at % 1000) * 1_000_000) as i32,
            }),
        };

        Ok(Response::new(entity))
    }
    // ... other stubs unchanged
}
```

### 9. Demo the compile-time check

Change the SQL to `SELECT id, structure_typo, ...` (typo). `cargo build` fails:

```
error: error returned from database: (code: 1) no such column: structure_typo
```

Restore the spelling.

### 10. Seed a row & smoke-test

```bash
sqlite3 server/calcifer.db <<SQL
INSERT INTO entities VALUES (
  'test-1', 'Note', 'Hello from SQLite',
  strftime('%s', 'now') * 1000, strftime('%s', 'now') * 1000
);
SQL

grpcurl -plaintext -d '{"id":"test-1"}' \
  -import-path proto -proto calcifer/v1/services.proto \
  localhost:8080 calcifer.v1.EntityService/Get
```

Should return the seeded entity.

## Verification

1. `cargo build` clean; misspelled column blocks build.
2. `sqlx migrate run` applies the migration; `sqlite3 server/calcifer.db ".schema"` shows all 5 tables.
3. `cargo run` boots; manually-seeded row retrieved via `grpcurl`.
4. Asking for a nonexistent ID returns `NOT_FOUND`.

## Done when

All 4 verification steps pass; you can articulate the difference between `query!` (compile-time check, returns anonymous struct) and `query_as!` (typed result struct).

## What's next (Part 4)

Implement `Create`, `Update`, `Delete`, plus the `properties` table. This is where `PropertyValue` gets prost-encoded into a BLOB column.
