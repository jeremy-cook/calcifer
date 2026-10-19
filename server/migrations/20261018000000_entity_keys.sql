-- ADR 10: natural keys declared by the structure registry (`KeyDef` in
-- server/src/structures.rs) live in one table, replacing the per-structure
-- `date_key` column and the `one_daily_note_per_day` and `one_tag_per_name`
-- partial indexes. `structure_type` is copied here because a unique constraint
-- can't join to `entities`; an entity's structure never changes, so it can't
-- drift. `value` is canonical: an ISO day, or a name trimmed and lowercased.
CREATE TABLE entity_keys (
  structure_type TEXT NOT NULL,
  key            TEXT NOT NULL,   -- 'date', 'name'
  value          TEXT NOT NULL,   -- canonical: '2026-10-10', 'reading'
  entity_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  PRIMARY KEY (structure_type, key, value),
  UNIQUE (entity_id, key)
);

INSERT INTO entity_keys (structure_type, key, value, entity_id)
SELECT 'DailyNote', 'date', date_key, id
FROM entities
WHERE structure_type = 'DailyNote' AND date_key IS NOT NULL;

-- SQLite's lower() only folds ASCII, while the server folds with Rust's
-- `str::to_lowercase`, so a non-ASCII Tag name written before this migration
-- could get a key that doesn't match the server's folding. Accepted: the local
-- database had no Tags at last check.
INSERT INTO entity_keys (structure_type, key, value, entity_id)
SELECT 'Tag', 'name', lower(trim(name)), id
FROM entities
WHERE structure_type = 'Tag';

DROP INDEX one_daily_note_per_day;
DROP INDEX one_tag_per_name;

-- After its index is dropped: SQLite refuses to drop an indexed column.
ALTER TABLE entities DROP COLUMN date_key;
