-- What changed on a run, and when, kept beside the count of writes.
--
-- `session_write` (0054) logs only that a run was written, once per write, so
-- the search index can ask whether anything changed. The run's own row keeps
-- the state it holds NOW: a focus overwrites the one before, a zone the zone
-- before, a close the moment it happened. This keeps the changes themselves.
-- The row stays the projection and answers every current read as before.
--
-- `changed` names the change (begin, entry, amend_last, amend_beat, focus,
-- timezone, stated_day, wrap_window, state, served_chars, served_reset, key,
-- charter_told). `value` is what was kept of it: the focus, the zone, the day,
-- the state moved to, the id of the entry written, the characters served. The
-- key a run works through is recorded as changed and never with its hash, and a
-- wrap window as offered or open and never with its code: both open a run and
-- neither is read back. `written_at` is the moment on the server's own clock.
--
-- A row appended before this has all three NULL. It says only that the run was
-- written, and nothing is backfilled: the only values to fill it with would be
-- guesses.
--
-- The count the search index reads is taken over the changes the index reads,
-- and the rows that predate this, so a change nothing searches for is recorded
-- and moves no signal.
--
-- One statement, so the runner can ask whether it landed.
ALTER TABLE session_write
    ADD COLUMN written_at VARCHAR(40) NULL,
    ADD COLUMN changed    VARCHAR(32) NULL,
    ADD COLUMN value      TEXT        NULL;
