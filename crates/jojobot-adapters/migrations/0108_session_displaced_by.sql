-- The run that took this run's role over, when that is why the run was closed.
--
-- A same-bot claim abandons the run that holds the role. The column is written
-- in the statement that closes the run, so a run closed by the sweep has none.
-- The value is the displacing run's public name. NULL on every row written
-- before this migration, and on a run that was reopened.
ALTER TABLE session ADD COLUMN displaced_by VARCHAR(16) NULL;
