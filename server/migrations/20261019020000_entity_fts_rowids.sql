-- Address an entity's `entity_fts` row by rowid. FTS5 can't look up an
-- UNINDEXED column, so `WHERE entity_id = ?` scanned the whole index on every
-- name and body write. Each entity gets a stable integer here that is its
-- `entity_fts` rowid. `entities.rowid` can't be it: `entities` has a TEXT
-- primary key, so VACUUM may renumber its implicit rowids.
CREATE TABLE entity_fts_rowids (
  fts_rowid  INTEGER PRIMARY KEY,   -- the entity's entity_fts rowid; stable across VACUUM
  entity_id  TEXT NOT NULL UNIQUE REFERENCES entities(id) ON DELETE CASCADE
);

INSERT INTO entity_fts_rowids (entity_id)
SELECT id FROM entities ORDER BY created_at, id;

-- Rebuild the index so each row's rowid is its entity's fts_rowid. `entity_id`
-- stays, UNINDEXED, so search reads a hit's id without a join. Rows for
-- entities that no longer exist are dropped, and an entity with no row gets
-- its name and an empty body.
CREATE VIRTUAL TABLE entity_fts_new USING fts5(
  entity_id UNINDEXED,
  name,
  body,
  tokenize = 'unicode61'
);

INSERT INTO entity_fts_new (rowid, entity_id, name, body)
SELECT m.fts_rowid, m.entity_id, coalesce(o.name, e.name), coalesce(o.body, '')
FROM entity_fts_rowids m
JOIN entities e ON e.id = m.entity_id
LEFT JOIN (
  -- One row per entity, in case the old delete-then-insert ever left two.
  SELECT entity_id, max(rowid) AS old_rowid FROM entity_fts GROUP BY entity_id
) latest ON latest.entity_id = m.entity_id
LEFT JOIN entity_fts o ON o.rowid = latest.old_rowid;

DROP TABLE entity_fts;
ALTER TABLE entity_fts_new RENAME TO entity_fts;
