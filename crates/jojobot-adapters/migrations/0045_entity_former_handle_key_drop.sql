-- The old key, `PRIMARY KEY (former_handle)`, alone allowed exactly one
-- event per former handle, ever. It comes off here so the next migration
-- can put the composite key `(former_handle, ordinal)` in its place —
-- MySQL refuses `ADD PRIMARY KEY` while one already stands, so the drop
-- and the add cannot be one clause.
--
-- The table carries no primary key between this migration and the next
-- one. Nothing else references this table by foreign key, and both
-- migrations run before this process serves a single request.
ALTER TABLE entity_former_handle DROP PRIMARY KEY;
