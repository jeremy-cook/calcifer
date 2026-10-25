-- ADR 12: direct relationships. A relationship is a one-way association from one
-- entity to another, written by AddRelationship / RemoveRelationship and limited
-- by the source structure's `relates_to` (server/src/structures.rs). The server
-- derives a link from each row under the reserved scope '@related'; this table is
-- the source, `links` the index.
CREATE TABLE relationships (
  entity_id  TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  target_id  TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (entity_id, target_id)
);
CREATE INDEX relationships_target ON relationships(target_id);
