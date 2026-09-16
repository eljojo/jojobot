-- Why an entity was archived — an entity's own analogue of a fact's
-- `status = 'archived'`. Free text, never a fixed category.
--
-- NULL is the ordinary case, and stays NULL on every row written before
-- this column existed.
ALTER TABLE entity ADD COLUMN archived_reason TEXT NULL;
