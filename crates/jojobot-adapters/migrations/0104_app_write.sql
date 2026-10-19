-- The writes an app made, one row each, and what each one landed.
--
-- An app sends an attempt id with every write. The row is inserted in the SAME
-- transaction as the write it records, so a write and its row land together or
-- not at all. The primary key makes two calls with one attempt from one app
-- race to one winner: the second insert fails and its whole write rolls back,
-- and the same call sent again finds the row and answers from it.
--
-- `app_id` is the app's configured name. `event_key` is the attempt id the app
-- sent. `digest` is a digest of the body the first call sent, so a repeat can
-- say whether its body differed. `landed` holds what the store returned for
-- the write, as JSON.
--
-- A row is never removed. An app's retry may come after any delay, and a row
-- removed would let the same attempt write a second event.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE app_write (
    app_id     VARCHAR(32)  NOT NULL,
    event_key  VARCHAR(128) NOT NULL,
    digest     VARCHAR(64)  NOT NULL,
    landed     TEXT         NOT NULL,
    written_at VARCHAR(48)  NOT NULL,
    PRIMARY KEY (app_id, event_key)
);
