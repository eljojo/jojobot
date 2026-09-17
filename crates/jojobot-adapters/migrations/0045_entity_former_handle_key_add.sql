-- The composite key, in place of the single-column one the previous
-- migration dropped. `ordinal` is this table's own per-key sequence, so the
-- primary key moving to `(former_handle, ordinal)` is what lets every
-- rename event survive rather than colliding on a handle that has been
-- reused.
ALTER TABLE entity_former_handle ADD PRIMARY KEY (former_handle, ordinal);
