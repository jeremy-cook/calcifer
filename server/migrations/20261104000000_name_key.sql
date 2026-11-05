-- I-83: names compare one way. `name_key` is the name folded by `fold_name`
-- (server/src/structures.rs): trimmed and lowercased with Unicode case folding.
-- It replaces `entities_name_ci`, whose `NOCASE` folds only ASCII.
--
-- SQLite can't fold non-ASCII, so the backfill below is exact only for ASCII
-- names; `db::refold_names`, run after the migrations on every boot, refolds
-- each row whose key isn't `fold_name(name)`. The key is BINARY, so a prefix
-- match (`structure_type = ? AND name_key >= ? AND name_key < ?`) is a range
-- on the index.
ALTER TABLE entities ADD COLUMN name_key TEXT NOT NULL DEFAULT '';
UPDATE entities SET name_key = lower(trim(name));
DROP INDEX entities_name_ci;
CREATE INDEX entities_name_key ON entities(structure_type, name_key);
