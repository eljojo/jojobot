-- The day a resume most recently set this run to.
--
-- started_on is the day the run began, and never moves. This is the day a
-- later resume moved it to, read ahead of started_on whenever both exist —
-- so a restart keeps the day a resume set, the same way it already keeps
-- the zone.
--
-- NULL is a run no resume has moved. Those read as started_on, which is
-- what they already meant.
ALTER TABLE session ADD COLUMN stated_day VARCHAR(10) NULL;
