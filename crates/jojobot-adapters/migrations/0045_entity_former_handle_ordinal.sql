-- Rule 209, made safe to write. A former handle keeps resolving even after
-- it has been reused — and nothing anywhere reserves a handle a rename
-- vacates, so reuse is ordinary rather than contrived: renaming A to B,
-- then B back to A, then A to B again writes three events under two
-- former handles, one of them twice; a handle vacated by one thing and
-- later claimed and renamed away by a different thing writes two events
-- under the same former handle for two different badges. The primary key
-- on `former_handle` alone allowed exactly one event per former handle,
-- ever — the second write in either sequence hit a duplicate-key error.
--
-- `ordinal` is this table's own per-key sequence, the same shape
-- `field_write` and `fact_stands_for` already keep: the primary key moves
-- to (former_handle, ordinal), so every event is kept, and a lookup reads
-- the highest ordinal for a former handle to get the newest one — the same
-- newest-write-wins rule every other repeated write in this store follows.
--
-- `former_handle` is also widened from VARCHAR(64) to VARCHAR(191): the
-- domain accepts a handle up to 128 characters, and 191 is the width every
-- other indexed handle-or-badge column already uses (`entity.id`,
-- `fact.edge_object`). At 64, a rename to or from a handle over that length
-- failed outright under the store's strict mode.
ALTER TABLE entity_former_handle
    MODIFY COLUMN former_handle VARCHAR(191) NOT NULL,
    ADD COLUMN ordinal INT NOT NULL DEFAULT 1,
    DROP PRIMARY KEY,
    ADD PRIMARY KEY (former_handle, ordinal);
