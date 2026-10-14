-- Every attempt at a push, in the order it happened.
--
-- `push_outbox` keeps the state a push holds NOW: the count of failures and the
-- moment of the next try, overwritten by each failure. This keeps the attempts
-- themselves, one row each, so an earlier failure and the moment it promised
-- can be read back. `push_outbox` stays the projection and answers every
-- current read as before.
--
-- `outcome` is failed, sent or abandoned. `attempted_at` is the moment the
-- outcome was recorded. `next_attempt_at` is the moment a failed attempt
-- promised the next try, and NULL for an attempt that ended the push.
--
-- A push attempted before this existed has no row here. Absence says nothing
-- was recorded, never that nothing happened, and nothing is backfilled.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE push_attempt (
    message_id      VARCHAR(64) NOT NULL,
    ordinal         INT         NOT NULL,
    attempted_at    VARCHAR(40) NOT NULL,
    outcome         VARCHAR(16) NOT NULL,
    next_attempt_at VARCHAR(40) NULL,
    PRIMARY KEY (message_id, ordinal)
);
