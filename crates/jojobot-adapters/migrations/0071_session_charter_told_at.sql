-- When a run was last handed its bot's charter. A charter another identity
-- rewrote after this moment has not been read by the run, so the status bar
-- tells it. Kept in the store because a run outlives a restart.
--
-- NULL is a run that was handed the charter only at its start, and every row
-- written before this column: the run's start is then the moment to compare
-- from, so no row is rewritten.
ALTER TABLE session ADD COLUMN charter_told_at VARCHAR(40) NULL;
