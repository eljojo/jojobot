-- The pushes waiting to go out: one row per message that landed in a person's
-- box, written in the same transaction that stores the message.
--
-- **A row holds no text.** What is pushed is read from the message when it is
-- sent, so a handle written into a body reads as the handle it answers to then,
-- and the body is stored once.
--
-- **A table of its own, not a column on `message`**: the migration runner asks
-- whether the object a migration creates is there, and a table is the shape that
-- question can answer.
--
-- `state` holds the token (`pending`, `sent`, `abandoned`) for the reason
-- `message.state` does. Timestamps are text, written at one fixed width by the
-- adapter and parsed by a reader, because a column cannot hold nanoseconds and a
-- reader never compares them as text.
CREATE TABLE push_outbox (
    message_id      VARCHAR(64) NOT NULL PRIMARY KEY,
    state           VARCHAR(12) NOT NULL,
    enqueued_at     VARCHAR(48) NOT NULL,
    attempts        INT         NOT NULL,
    next_attempt_at VARCHAR(48) NOT NULL,
    finished_at     VARCHAR(48) NULL,
    INDEX by_state (state)
);
