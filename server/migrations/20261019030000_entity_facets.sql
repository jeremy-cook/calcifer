-- ADR 11: which facets an entity has attached. A facet is declared in the
-- registry (`FacetDef` in server/src/structures.rs); its values live in
-- `properties` under namespaced ids ('todo.status'), not here.
CREATE TABLE entity_facets (
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  facet     TEXT NOT NULL,   -- 'todo'
  PRIMARY KEY (entity_id, facet)
);
