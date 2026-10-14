-- Column 3 of 3 of the session-write history. The first migration of the
-- set, 0084_session_write_history, carries the account of the whole set.
ALTER TABLE session_write ADD COLUMN value TEXT NULL;
