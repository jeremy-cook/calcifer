# Phase 8 — Part 4: Entity write path

Part of `app-plan.md` Phase 8. Full entity lifecycle — `Create`, `List`, `Update`, `Delete` — including the `properties` table.

## Goal

Round-trip an `Entity` through SQLite: write properties, read them back as a typed `PropertyValue` oneof. The trick is encoding `PropertyValue` as a prost-serialized BLOB so a single column can hold any oneof variant.

## What exists after Part 3

- `Get` reads the `entities` row but always returns `properties: vec![]`.
- `Create`/`Update`/`Delete` are still `Status::unimplemented` stubs.

## Steps

### 1. `Create`

```rust
async fn create(&self, req: Request<CreateEntityRequest>) -> Result<Response<Entity>, Status> {
    let entity = req.into_inner().entity
        .ok_or_else(|| Status::invalid_argument("missing entity"))?;

    let now = chrono::Utc::now().timestamp_millis();

    let mut tx = self.pool.begin().await.map_err(AppError::from)?;

    sqlx::query!(
        "INSERT INTO entities (id, structure_type, name, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
        entity.id, entity.structure_type, entity.name, now, now,
    )
    .execute(&mut *tx)
    .await
    .map_err(AppError::from)?;

    for prop in &entity.properties {
        let value = prop.value.as_ref()
            .ok_or_else(|| Status::invalid_argument("property missing value"))?;
        let blob = prost::Message::encode_to_vec(value);

        sqlx::query!(
            "INSERT INTO properties (entity_id, property_id, value_blob)
             VALUES (?, ?, ?)",
            entity.id, prop.id, blob,
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;
    }

    tx.commit().await.map_err(AppError::from)?;

    // Re-read so created_at/updated_at echo back consistently.
    self.load_entity(&entity.id).await
        .map(Response::new)
        .map_err(Status::from)
}
```

`load_entity` is a private helper we extract — same shape as `Get`'s body — that hydrates everything. Refactor `Get` to call it too.

### 2. Hydrate properties on read

`load_entity` (in an `impl EntityService` block, *not* the trait impl):

```rust
impl EntityService {
    async fn load_entity(&self, id: &str) -> Result<Entity, AppError> {
        let row = sqlx::query!(
            "SELECT id, structure_type, name, created_at, updated_at
             FROM entities WHERE id = ?",
            id
        )
        .fetch_optional(&self.pool).await?
        .ok_or_else(|| AppError::NotFound(format!("entity {}", id)))?;

        let prop_rows = sqlx::query!(
            "SELECT property_id, value_blob FROM properties WHERE entity_id = ?",
            id
        )
        .fetch_all(&self.pool).await?;

        let properties = prop_rows.into_iter().map(|r| {
            let value: PropertyValue = prost::Message::decode(&*r.value_blob)?;
            Ok::<_, AppError>(Property {
                id: r.property_id,
                value: Some(value),
            })
        }).collect::<Result<Vec<_>, _>>()?;

        Ok(Entity {
            id: row.id,
            structure_type: row.structure_type,
            name: row.name,
            properties,
            links: vec![],            // Part 5
            referenced_dates: vec![], // Part 5
            created_at: Some(ts_from_millis(row.created_at)),
            updated_at: Some(ts_from_millis(row.updated_at)),
        })
    }
}

fn ts_from_millis(millis: i64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: millis / 1000,
        nanos: ((millis % 1000) * 1_000_000) as i32,
    }
}
```

### 3. `List`

```rust
async fn list(&self, req: Request<ListEntitiesRequest>) -> Result<Response<ListEntitiesResponse>, Status> {
    let filter = req.into_inner().structure_type;

    let ids: Vec<String> = if filter.is_empty() {
        sqlx::query_scalar!("SELECT id FROM entities ORDER BY updated_at DESC")
            .fetch_all(&self.pool).await
    } else {
        sqlx::query_scalar!(
            "SELECT id FROM entities WHERE structure_type = ? ORDER BY updated_at DESC",
            filter
        )
        .fetch_all(&self.pool).await
    }.map_err(AppError::from)?;

    let mut entities = Vec::with_capacity(ids.len());
    for id in ids {
        entities.push(self.load_entity(&id).await.map_err(Status::from)?);
    }

    Ok(Response::new(ListEntitiesResponse { entities }))
}
```

`query_scalar!` is the variant when you select a single column.

### 4. `Update`

Replace properties wholesale inside a transaction, mirroring Part 3's FE pattern:

```rust
async fn update(&self, req: Request<UpdateEntityRequest>) -> Result<Response<Entity>, Status> {
    let entity = req.into_inner().entity
        .ok_or_else(|| Status::invalid_argument("missing entity"))?;
    let now = chrono::Utc::now().timestamp_millis();

    let mut tx = self.pool.begin().await.map_err(AppError::from)?;

    sqlx::query!(
        "UPDATE entities SET structure_type = ?, name = ?, updated_at = ? WHERE id = ?",
        entity.structure_type, entity.name, now, entity.id,
    ).execute(&mut *tx).await.map_err(AppError::from)?;

    sqlx::query!("DELETE FROM properties WHERE entity_id = ?", entity.id)
        .execute(&mut *tx).await.map_err(AppError::from)?;

    for prop in &entity.properties {
        let value = prop.value.as_ref()
            .ok_or_else(|| Status::invalid_argument("property missing value"))?;
        let blob = prost::Message::encode_to_vec(value);
        sqlx::query!(
            "INSERT INTO properties (entity_id, property_id, value_blob)
             VALUES (?, ?, ?)",
            entity.id, prop.id, blob,
        ).execute(&mut *tx).await.map_err(AppError::from)?;
    }

    tx.commit().await.map_err(AppError::from)?;

    self.load_entity(&entity.id).await.map(Response::new).map_err(Status::from)
}
```

### 5. `Delete`

```rust
async fn delete(&self, req: Request<DeleteEntityRequest>) -> Result<Response<()>, Status> {
    let id = req.into_inner().id;

    let mut tx = self.pool.begin().await.map_err(AppError::from)?;

    // ON DELETE CASCADE handles properties / links / referenced_dates.
    sqlx::query!("DELETE FROM entities WHERE id = ?", id)
        .execute(&mut *tx).await.map_err(AppError::from)?;

    // richtext has no FK reference — purge explicitly.
    sqlx::query!("DELETE FROM richtext WHERE entity_id = ?", id)
        .execute(&mut *tx).await.map_err(AppError::from)?;

    tx.commit().await.map_err(AppError::from)?;
    Ok(Response::new(()))
}
```

## Verification

1. `cargo build` clean.
2. `grpcurl ... Create` with a body containing one text property succeeds; `sqlite3` shows rows in both `entities` and `properties`.
3. `grpcurl ... Get` round-trips that property — the response JSON shows the `text` oneof variant correctly.
4. `grpcurl ... List` with no filter returns the entity; `List` with `{"structure_type":"Note"}` returns only Notes.
5. `grpcurl ... Update` replaces the property; `properties` table reflects the new row.
6. `grpcurl ... Delete` removes the entity; `SELECT COUNT(*) FROM properties WHERE entity_id = '<id>'` returns 0.

## Done when

All 6 verification steps pass; you can explain why we encode `PropertyValue` as a single BLOB instead of one column per `oneof` variant.

## What's next (Part 5)

Persist `links` and `referenced_dates` so backlinks and the calendar references panel will work once the FE is rewired.
