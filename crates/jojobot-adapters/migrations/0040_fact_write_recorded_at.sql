-- The same name on the substrate, because every write carries the whole claim.
--
-- A claim is a projection over its writes, so the two tables hold the same
-- columns; a name that moved on one and not the other would be one row shape
-- reading two ways.
ALTER TABLE fact_write RENAME COLUMN date TO recorded_at;
