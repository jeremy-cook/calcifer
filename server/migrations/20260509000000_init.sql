CREATE TABLE entities (
  id              TEXT PRIMARY KEY,
  structure_type  TEXT NOT NULL,
  name            TEXT NOT NULL,
  created_at      INTEGER NOT NULL,    -- unix millis
  updated_at      INTEGER NOT NULL
);

-- Case-insensitive lookup for ResolveByName (get-or-create by name).
CREATE INDEX entities_name_ci ON entities(structure_type, name COLLATE NOCASE);

-- ADR 10: natural keys declared by the structure registry (`KeyDef` in
-- server/src/structures.rs). `structure_type` is copied here because a unique
-- constraint can't join to `entities`; an entity's structure never changes, so
-- it can't drift. `value` is canonical: an ISO day, or a name trimmed and
-- lowercased.
CREATE TABLE entity_keys (
  structure_type TEXT NOT NULL,
  key            TEXT NOT NULL,   -- 'date', 'name'
  value          TEXT NOT NULL,   -- canonical: '2026-10-10', 'reading'
  entity_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  PRIMARY KEY (structure_type, key, value),
  UNIQUE (entity_id, key)
);

CREATE TABLE properties (
  entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  property_id  TEXT NOT NULL,
  value_blob   BLOB NOT NULL,           -- prost-encoded PropertyValue
  PRIMARY KEY (entity_id, property_id)
);

CREATE TABLE richtext (
  entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
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

-- Full-text index over note names + bodies for lexical search, kept in sync
-- inside the same transactions as EntityService create/update/delete (name) and
-- RichTextService.Put (body). Each entity's row is addressed by rowid, because
-- FTS5 can't look up an UNINDEXED column; `entity_id` stays, UNINDEXED, so
-- search reads a hit's id without a join.
CREATE VIRTUAL TABLE entity_fts USING fts5(
  entity_id UNINDEXED,
  name,
  body,
  tokenize = 'unicode61'
);

-- Each entity's stable `entity_fts` rowid. `entities.rowid` can't be it:
-- `entities` has a TEXT primary key, so VACUUM may renumber its implicit rowids.
CREATE TABLE entity_fts_rowids (
  fts_rowid  INTEGER PRIMARY KEY,   -- the entity's entity_fts rowid; stable across VACUUM
  entity_id  TEXT NOT NULL UNIQUE REFERENCES entities(id) ON DELETE CASCADE
);

-- Semantic-retrieval storage: chunked content + their vector embeddings.
--
-- `chunks` holds the plain-text slices of each entity's richtext docs, keyed by a
-- stable (entity_id, chunk_index). `entity_vec` is a sqlite-vec `vec0` virtual
-- table holding one embedding per chunk, joined back to `chunks` by a shared
-- integer rowid. Both are owned by the background embed worker (see
-- server/src/embed), which delete-then-inserts an entity's rows wholesale on each
-- Put/Create/Update.
--
-- The embedding dimension is PINNED at 384 to match the local all-MiniLM-L6-v2
-- model (fastembed). Changing the embedder's dimension requires a new migration
-- that recreates `entity_vec` with the new float[N] width.

CREATE TABLE chunks (
  id           INTEGER PRIMARY KEY,      -- shared rowid with entity_vec
  entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  chunk_index  INTEGER NOT NULL,         -- 0-based position within the entity's content
  text         TEXT NOT NULL,
  UNIQUE (entity_id, chunk_index)
);

-- vec0 virtual table: one 384-d float embedding per chunk row, addressed by the
-- same integer key as `chunks.id`. KNN is `embedding MATCH ? ORDER BY distance`.
-- It can't cascade from `chunks`, so DeleteEntity deletes an entity's
-- `entity_vec` rows before the entity.
CREATE VIRTUAL TABLE entity_vec USING vec0(
  id INTEGER PRIMARY KEY,
  embedding float[384]
);
