-- A run's public name is six characters, and the teaching ledger is keyed by it.
--
-- `session_teaching.sid` was four characters wide because a run's handle was.
-- A run now carries a public name minted at its boot, and that name is part of
-- this table's primary key, so the column is widened. Existing rows keep the
-- four-character handles they hold.
ALTER TABLE session_teaching MODIFY sid VARCHAR(64) NOT NULL;
