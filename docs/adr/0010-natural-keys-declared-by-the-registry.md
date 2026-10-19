# 10. Natural keys are declared by the registry and stored in `entity_keys`

- **Status:** Accepted
- **Date:** 2026-10-18

## Context

Some entities have a natural key: a value that identifies them besides their id. A
DailyNote is the note for one day, and a Tag is the Tag with one name. Each was enforced
on its own:

- **DailyNote:** `entities.date_key` copied the `date` property, and the partial unique
  index `one_daily_note_per_day` allowed one DailyNote per day. `date_key_for` and
  `date_key_from` in `services/entity.rs` kept the copy in sync on Create, Resolve and
  SetProperty ([ADR 4](0004-dates-without-a-dateref-entity.md)).
- **Tag:** the partial unique index `one_tag_per_name` was on `name COLLATE NOCASE`.
  `NOCASE` only folds ASCII, so `Café` and `CAFÉ` were two Tags.

Both indexes named a structure in a migration, each copy needed special-case code on
every write path, and `map_unique_violation` matched on the column each one reported.
Property values are prost blobs in `properties`, so SQLite can't index them directly.
The registry ([ADR 7](0007-server-owned-structure-registry.md)) is meant to be the one
place structure data is authored, but a new keyed structure would have needed a
migration and more special cases.

## Decision

**The registry declares each structure's natural keys, and one generic table enforces
them.**

`StructureDef` gains `keys: &'static [KeyDef]`. A `KeyDef` names the key and its
source: the entity's name (`KeySource::Name`, trimmed and lowercased with Rust's
`str::to_lowercase`, which folds Unicode) or a property (`KeySource::Property(id)`,
whose value is already canonical once `validate_property` has passed it). Tag declares
`name` from its name; DailyNote declares `date` from its `date` property.

```sql
CREATE TABLE entity_keys (
  structure_type TEXT NOT NULL,
  key            TEXT NOT NULL,
  value          TEXT NOT NULL,   -- canonical
  entity_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  PRIMARY KEY (structure_type, key, value),
  UNIQUE (entity_id, key)
);
```

- The primary key gives a key value to at most one entity of a structure.
  `structure_type` is copied from `entities` because a unique constraint can't join;
  an entity's structure never changes, so the copy can't drift.
- `UNIQUE (entity_id, key)` gives an entity at most one value per key.
- Every write that can change a key's source rewrites its row in the same
  transaction: Create and Resolve write every declared key, Rename the name keys,
  SetProperty the keys sourced from that property. A taken value is `ALREADY_EXISTS`.
- Lookups use the same table and folding: Resolve by date, and Resolve by name for a
  structure with a name key. Other structures keep the `entities_name_ci` lookup.
- `unique_names` stays on the wire, derived: a structure has unique names if it
  declares a name key.

The table enforces identity only. Types, select options and relation targets are still
checked by `validate_property`.

## Alternatives considered

- **Keep the `date_key` column, and add one per future key.** Each new key is a
  migration, a partial index and special-case code on every write path. This is what
  the change replaces.
- **A single `natural_key` column on `entities`.** One column and one
  `(structure_type, natural_key)` unique index would serve Tag and DailyNote, but an
  entity could have only one key, and the column means something different for each
  structure.
- **Store properties as JSON and index an expression.** A unique index on
  `json_extract(properties, '$.date')` would read the value in place, but changing how
  property values are stored is a much larger change, and each index would still name a
  structure in a migration.
- **Typed value columns** (`value_text`, `value_date`, …) in `properties`, with partial
  unique indexes. The same storage rewrite, and still one index per keyed property.
- **The date as the DailyNote's id.** It makes per-day uniqueness free, but a DailyNote
  could no longer move to another day without changing its id and every reference to
  it, and ids would stop being uniform uuids.
- **Drop the `date` property and keep the date only in the key.** The date is shown and
  edited as a property like any other; making it a special column again moves the
  special case rather than removing it.
- **A table per structure** (`daily_notes(date UNIQUE)`, `tags(name UNIQUE)`). Plain
  constraints, but it brings back the per-kind schema that
  [ADR 1](0001-polymorphic-entity-model.md) decided against, and a new structure needs
  a migration.

## Consequences

- Adding a keyed structure, or a key to an existing one, is a registry edit with no
  migration.
- Tag names now fold Unicode case: `Café` and `CAFÉ` conflict, and a Tag resolves by
  name under any casing.
- This replaces the uniqueness mechanism in ADR 4's consequences (the `date_key` column
  and `one_daily_note_per_day` index). It also makes the "advisory" `uniqueNames` of
  ADR 1 a hard rule, enforced by the database on Create, Rename and Resolve.
- The key value is a second copy of the name or property, kept in sync by the write
  paths. A write path that changes a key's source without calling `set_key` would let
  it drift, so every such path lives in `services/entity.rs`.
- The backfill folded existing Tag names with SQLite's `lower()`, which only folds
  ASCII. A non-ASCII Tag written before the migration could have a key that doesn't
  match the server's folding. The local database had no Tags when this was written.
- The rule that a DailyNote's name is derived from its date, and isn't editable, is
  still special-cased in `services/entity.rs`. Moving it into the registry is separate.
