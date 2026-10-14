-- The index on the hash of the key a run holds.
--
-- 0074 added the key_hash column and this index in one ALTER, and its landed
-- check names the column only. A store whose ledger holds 0074 and whose index
-- is missing was never asked to build it. This migration has its own landed
-- check, which names the index, so a start builds the index when it is missing
-- and records the version without issuing it when the index stands. 0074 is not
-- edited: a ledger that records it records its text.
ALTER TABLE session ADD INDEX by_key_hash (key_hash);
