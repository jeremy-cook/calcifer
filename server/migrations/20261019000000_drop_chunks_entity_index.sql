-- `chunks`' UNIQUE (entity_id, chunk_index) already gives it an index that
-- starts with entity_id (sqlite_autoindex_chunks_1), so this one only added
-- write cost to every embed-worker write.
DROP INDEX chunks_entity;
