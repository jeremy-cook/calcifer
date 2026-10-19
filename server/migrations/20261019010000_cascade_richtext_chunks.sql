-- `richtext` and `chunks` cascade from `entities`, like `properties` and
-- `links`, so deleting an entity removes its documents and chunks and no row
-- can outlive its entity. SQLite can't add a foreign key with ALTER TABLE, so
-- each table is rebuilt: create `<table>_new` with the foreign key, copy the
-- rows that still have an entity, drop the old table and rename.
--
-- `entity_vec` (vec0) can't cascade: it's a virtual table keyed by `chunks.id`.
-- The copy keeps every `chunks.id`, so existing embeddings stay joined, and
-- DeleteEntity deletes an entity's `entity_vec` rows before the entity.

DELETE FROM richtext WHERE entity_id NOT IN (SELECT id FROM entities);

CREATE TABLE richtext_new (
  entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  property_id  TEXT NOT NULL,
  doc          TEXT NOT NULL,           -- TipTap JSON
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (entity_id, property_id)
);
INSERT INTO richtext_new (entity_id, property_id, doc, updated_at)
SELECT entity_id, property_id, doc, updated_at FROM richtext;
DROP TABLE richtext;
ALTER TABLE richtext_new RENAME TO richtext;

-- An orphaned chunk's embedding goes with it.
DELETE FROM entity_vec
WHERE id IN (SELECT id FROM chunks WHERE entity_id NOT IN (SELECT id FROM entities));
DELETE FROM chunks WHERE entity_id NOT IN (SELECT id FROM entities);

CREATE TABLE chunks_new (
  id           INTEGER PRIMARY KEY,      -- shared rowid with entity_vec
  entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  chunk_index  INTEGER NOT NULL,         -- 0-based position within the entity's content
  text         TEXT NOT NULL,
  UNIQUE (entity_id, chunk_index)
);
INSERT INTO chunks_new (id, entity_id, chunk_index, text)
SELECT id, entity_id, chunk_index, text FROM chunks;
DROP TABLE chunks;
ALTER TABLE chunks_new RENAME TO chunks;
