-- I-22: a rich-text document is addressed by (entity id, declared property id),
-- so rich-text properties no longer store a RichTextRef in `properties`. Delete
-- the rows written before that change. `value_blob` is prost-encoded, so rows are
-- picked by the structure registry rather than by their value: the
-- (structure_type, property_id) pairs below are every property declared with
-- PropertyKind::Richtext in `STRUCTURES` in server/src/structures.rs (Note,
-- DailyNote and Todo each declare `content`). The `richtext` table, which holds
-- the documents themselves, is untouched. Only these rows are deleted.
DELETE FROM properties
WHERE EXISTS (
  SELECT 1
  FROM entities e
  WHERE e.id = properties.entity_id
    AND (e.structure_type, properties.property_id) IN (
      VALUES ('Note', 'content'), ('DailyNote', 'content'), ('Todo', 'content')
    )
);
