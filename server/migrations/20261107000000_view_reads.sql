-- ADR 9: views query the server. `value_key` makes a property's value
-- filterable and sortable in SQL, which the prost-encoded `value_blob` isn't:
-- canonical sortable text for scalar kinds (an ISO day, a select key, a text,
-- an order-preserving encoding of a number), null for relation values. Every
-- write sets it in the transaction that writes the blob
-- (`services::entity::value_key`). SQL can't decode the blob, so the backfill
-- is `db::fill_value_keys`, run after the migrations on every boot.
ALTER TABLE properties ADD COLUMN value_key TEXT;
CREATE INDEX properties_value_key ON properties(property_id, value_key);

-- A structure's list, most recently updated first.
CREATE INDEX entities_structure_updated ON entities(structure_type, updated_at);
