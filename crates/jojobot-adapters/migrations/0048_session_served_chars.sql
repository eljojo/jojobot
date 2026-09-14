-- Rule 264: what jojobot hands back across a session is what is budgeted,
-- and storage is never budgeted. Nothing today accumulates what a session
-- has been handed across many answers, so nothing can notice a ceiling
-- approaching. This is the running total, kept where the session's own
-- record already lives.
ALTER TABLE session ADD COLUMN served_chars BIGINT NOT NULL DEFAULT 0;
