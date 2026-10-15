-- The index on the hash of the newest retired key of a run.
--
-- It answers "which run did this key belong to" without reading every row.
-- Its own file, so its landed check names the index: a start that finds the
-- column and no index builds the index.
ALTER TABLE session ADD INDEX by_retired_key_hash (retired_key_hash);
