-- Semantic-retrieval storage (M8): chunked content + their vector embeddings.
--
-- `chunks` holds the plain-text slices of each entity's richtext docs, keyed by a
-- stable (entity_id, chunk_index). `entity_vec` is a sqlite-vec `vec0` virtual
-- table holding one embedding per chunk, joined back to `chunks` by a shared
-- integer rowid. Both are owned by the background embed worker (see
-- server/src/embed), which delete-then-inserts an entity's rows wholesale on each
-- Put/Create/Update — mirroring how RichTextService.Put owns `entity_fts`.
--
-- The embedding dimension is PINNED at 384 to match the local all-MiniLM-L6-v2
-- model (fastembed). Changing the embedder's dimension requires a new migration
-- that recreates `entity_vec` with the new float[N] width.

CREATE TABLE chunks (
  id           INTEGER PRIMARY KEY,      -- shared rowid with entity_vec
  entity_id    TEXT NOT NULL,
  chunk_index  INTEGER NOT NULL,         -- 0-based position within the entity's content
  text         TEXT NOT NULL,
  UNIQUE (entity_id, chunk_index)
);
CREATE INDEX chunks_entity ON chunks(entity_id);

-- vec0 virtual table: one 384-d float embedding per chunk row, addressed by the
-- same integer key as `chunks.id`. KNN is `embedding MATCH ? ORDER BY distance`.
CREATE VIRTUAL TABLE entity_vec USING vec0(
  id INTEGER PRIMARY KEY,
  embedding float[384]
);
