-- Case-insensitive lookup for ResolveByName (get-or-create by name).
CREATE INDEX entities_name_ci ON entities(structure_type, name COLLATE NOCASE);

-- Tag names are identity: forbid two Tags with the same (case-insensitive) name.
CREATE UNIQUE INDEX one_tag_per_name
  ON entities(name COLLATE NOCASE)
  WHERE structure_type = 'Tag';
