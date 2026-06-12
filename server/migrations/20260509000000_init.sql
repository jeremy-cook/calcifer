CREATE TABLE entities (
  id              TEXT PRIMARY KEY,
  structure_type  TEXT NOT NULL,
  name            TEXT NOT NULL,
  date_key        TEXT,                -- mirror of the DailyNote 'date' property; NULL for others
  created_at      INTEGER NOT NULL,    -- unix millis
  updated_at      INTEGER NOT NULL
);

-- "one DailyNote per calendar day" enforced in the DB, not app code.
CREATE UNIQUE INDEX one_daily_note_per_day
  ON entities(date_key)
  WHERE structure_type = 'DailyNote' AND date_key IS NOT NULL;

CREATE TABLE properties (
  entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  property_id  TEXT NOT NULL,
  value_blob   BLOB NOT NULL,           -- prost-encoded PropertyValue
  PRIMARY KEY (entity_id, property_id)
);

CREATE TABLE richtext (
  entity_id    TEXT NOT NULL,
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
