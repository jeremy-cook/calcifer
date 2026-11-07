-- Batch 3, search quality. `entity_fts` gains a `tags` column, the names of the
-- Tags an entity's properties hold, so a note is found by its Tags (I-87), and
-- the `porter` stemmer, so "engines" finds "engine" (I-104). Its `body` now reads
-- each chip as its target's current name (I-103). FTS5 can't change a table's
-- columns or tokenizer, so it's rebuilt empty: `search_index::index_missing`,
-- run after the migrations on every boot, fills it. `entity_fts_rowids` keeps
-- each entity's rowid.
DROP TABLE entity_fts;
CREATE VIRTUAL TABLE entity_fts USING fts5(
  entity_id UNINDEXED,
  name,
  tags,
  body,
  tokenize = 'porter unicode61'
);

-- The first embedding chunk now leads with the name and Tags, so every entity
-- is re-embedded: the embed worker queues each entity with no chunks when it
-- starts.
DELETE FROM entity_vec;
DELETE FROM chunks;
