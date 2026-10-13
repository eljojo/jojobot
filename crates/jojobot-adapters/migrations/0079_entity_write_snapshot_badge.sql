-- Column 12 of 13 in the entity-write snapshot. The first migration of the
-- set, 0079_entity_write_snapshot_kind, carries the account of the whole set.
ALTER TABLE entity_write ADD COLUMN badge VARCHAR(16) NULL;
