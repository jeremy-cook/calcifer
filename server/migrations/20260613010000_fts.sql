-- Full-text index over note names + bodies for lexical search (M7).
-- Kept in sync (delete-then-insert by entity_id) inside the same transactions as
-- EntityService create/update/delete (name) and RichTextService.Put (body).
CREATE VIRTUAL TABLE entity_fts USING fts5(
  entity_id UNINDEXED,
  name,
  body,
  tokenize = 'unicode61'
);

-- Backfill names for entities that predate this index so they are immediately
-- searchable. The `body` column starts empty and is populated by the first
-- RichTextService.Put per entity (TipTap-JSON -> plain text needs app logic).
INSERT INTO entity_fts (entity_id, name, body)
SELECT id, name, '' FROM entities;
