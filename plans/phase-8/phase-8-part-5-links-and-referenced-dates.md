# Phase 8 — Part 5: Links & referenced_dates

Part of `app-plan.md` Phase 8. Persist outgoing links and date references on every Create/Update so backlinks and the calendar references panel work once the FE is rewired in Part 8.

> **Amendment (post-0a data-model review).** Derivation of *richtext-sourced* links and dates moves to `RichTextService.Put` (see Part 6), because a doc save knows only its own property. That changes this part:
> - **`replace_links` here covers only non-richtext (relation-property) links** written via `EntityService.Update`. It must scope its delete per property — `DELETE FROM links WHERE entity_id = ? AND source_property_id = ?` — **not** the wholesale `DELETE FROM links WHERE entity_id = ?` shown below, or it would clobber the richtext-sourced links `Put` maintains. (Until relation properties exist, `Update` carries no links and this is a no-op.)
> - **`replace_referenced_dates` likewise moves to `Put`** as an entity-scoped union; `Update` no longer owns it.
> - The hydrate-on-read logic (step 3) is unchanged — it reads whatever rows exist regardless of who wrote them.

## Goal

Mirror what `linkSync.ts` already does on the FE: after every successful entity write, replace the entity's `links` and `referenced_dates` rows wholesale.

## Why "delete-all-then-insert" instead of diffing

The FE already computes the **full** set of outgoing links per save (it walks the TipTap doc and collects every mention/datechip). We get the complete target set for free. Diffing would require a separate "what changed" computation; delete-then-insert is one fewer thing to think about and is fast at the entity-scope counts we'll ever see (typically <100 links per entity).

## Steps

### 1. Extract a helper for replacing relational tail rows

Inside `impl EntityService`:

```rust
async fn replace_links(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    links: &[LinkRef],
) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM links WHERE entity_id = ?", entity_id)
        .execute(&mut **tx).await?;

    for link in links {
        let target = link.target.as_ref()
            .ok_or_else(|| AppError::Decode(prost::DecodeError::new("link missing target")))?;
        let created_at = link.created_at.as_ref()
            .map(|t| t.seconds * 1000 + (t.nanos as i64) / 1_000_000)
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        sqlx::query!(
            "INSERT INTO links (entity_id, link_id, target_id, target_structure, source_property_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            entity_id, link.id, target.id, target.structure_type,
            link.source_property_id, created_at,
        ).execute(&mut **tx).await?;
    }

    Ok(())
}

async fn replace_referenced_dates(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    dates: &[String],
) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM referenced_dates WHERE entity_id = ?", entity_id)
        .execute(&mut **tx).await?;

    for iso in dates {
        sqlx::query!(
            "INSERT INTO referenced_dates (entity_id, iso_date) VALUES (?, ?)",
            entity_id, iso,
        ).execute(&mut **tx).await?;
    }

    Ok(())
}
```

### 2. Call them from `Create` and `Update`

Inside the existing transactions in `create()` and `update()`, after the `properties` loop, before `tx.commit()`:

```rust
self.replace_links(&mut tx, &entity.id, &entity.links).await
    .map_err(Status::from)?;
self.replace_referenced_dates(&mut tx, &entity.id, &entity.referenced_dates).await
    .map_err(Status::from)?;
```

### 3. Hydrate on read

Extend `load_entity` in `impl EntityService` to also fetch links and referenced_dates:

```rust
let link_rows = sqlx::query!(
    "SELECT link_id, target_id, target_structure, source_property_id, created_at
     FROM links WHERE entity_id = ?",
    id
).fetch_all(&self.pool).await?;

let links = link_rows.into_iter().map(|r| LinkRef {
    id: r.link_id,
    target: Some(EntityRef {
        id: r.target_id,
        structure_type: r.target_structure,
    }),
    source_property_id: r.source_property_id,
    created_at: Some(ts_from_millis(r.created_at)),
}).collect();

let date_rows = sqlx::query_scalar!(
    "SELECT iso_date FROM referenced_dates WHERE entity_id = ?",
    id
).fetch_all(&self.pool).await?;

// ...then use `links` and `date_rows` in the Entity { ... } construction
Ok(Entity {
    // ... existing fields
    links,
    referenced_dates: date_rows,
    // ...
})
```

### 4. Backlinks

Backlinks aren't a separate RPC — they're derived on the FE via the existing `useBacklinks(entityId)` selector that scans `entity.links[]` across the entity store. With Part 8, that scan becomes a derivation off `useEntities()` cache data, so no server-side change needed *for the basic case*.

If we ever want server-side backlinks (e.g., paginated, or scaled past N entities), the query is:

```sql
SELECT entity_id FROM links WHERE target_id = ?
```

Add as `EntityService::ListBacklinks(EntityRef) returns (ListEntitiesResponse)` later if needed. **Not in scope for this part.**

## Verification

1. `cargo build` clean.
2. Create entity A. Create entity B with a `links[]` entry pointing at A's id. `SELECT * FROM links WHERE entity_id = '<B>';` shows the row; `target_id = '<A>'`.
3. `Get` on B returns `links: [...]` matching what was sent.
4. Update B with a different `links` set. `SELECT COUNT(*) FROM links WHERE entity_id = '<B>';` matches the new count (replace, not append).
5. Add a `referenced_dates: ["2026-05-09"]` to an entity on Create. `SELECT * FROM referenced_dates WHERE iso_date = '2026-05-09';` shows the row.
6. Backlinks query: `SELECT entity_id FROM links WHERE target_id = '<A>';` returns B's id.

## Done when

All 6 verification steps pass; the schema supports both directions of the mention graph for the FE to read in Part 8.

## What's next (Part 6)

Wire up `RichTextService` — the separate hot-path service for richtext docs.
