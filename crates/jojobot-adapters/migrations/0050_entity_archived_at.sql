-- When an entity was archived — stamped by the store, never a caller, the
-- same convention every other stamp on this store follows.
--
-- Both this column and archived_reason are set together on every write, and
-- a read treats one without the other as not archived rather than
-- inventing the missing half.
--
-- NULL is the ordinary case, and stays NULL on every row written before
-- this column existed.
ALTER TABLE entity ADD COLUMN archived_at VARCHAR(40) NULL;
