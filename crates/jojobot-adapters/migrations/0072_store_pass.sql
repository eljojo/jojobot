-- Which one-time passes over stored rows have finished on this store.
--
-- A pass that rewrites old rows onto permanent ids reads every row of a table.
-- Once it has finished on a store, a boot has nothing left to do for it, and a
-- read of the whole table would only grow with the store. The pass writes its
-- name here after it has examined every row without a failure, and a boot reads
-- these rows before it reads any table.
--
-- A pass that failed or was skipped writes nothing, so the next boot runs it
-- again. The pass itself is idempotent, so a row written twice is harmless.
CREATE TABLE store_pass (
    name        VARCHAR(64) NOT NULL,
    finished_at VARCHAR(40) NOT NULL,
    PRIMARY KEY (name)
);
