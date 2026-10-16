-- The parts of a write that commits in several, one row each.
--
-- A verb that writes in more than one commit (a document put in chunks, a bot
-- and then its box, a rename and then the repoint of its box) records each part
-- here in the SAME transaction as that part, so a part and its row land
-- together or not at all. A call that sends the same request id again skips
-- every part with a row, runs the rest, and returns the final receipt once the
-- last part is written. The final receipt is the row of `request_receipt`; this
-- table holds only the parts before it.
--
-- `bot_key` is the permanent identity of the bot that sent the id, never its
-- handle, as in `request_receipt`. `part` names the part inside the verb.
-- `args_digest` is a digest of the arguments the first call sent, so a repeat
-- can say whether its arguments differed. `landed` holds what the store
-- returned for the part (the ids it wrote) as JSON, and `note` holds what the
-- verb kept beside the part, such as the plan a repeat must finish; it is NULL
-- when the verb kept nothing.
--
-- The primary key makes two calls that write one part race to one winner: the
-- second insert fails and its whole part rolls back.
--
-- `written_at` is the moment on the server's own clock. Rows older than the
-- retention period are removed by the adapter.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE request_part (
    bot_key     VARCHAR(191) NOT NULL,
    request_id  VARCHAR(128) NOT NULL,
    part        VARCHAR(96)  NOT NULL,
    verb        VARCHAR(48)  NOT NULL,
    args_digest VARCHAR(64)  NOT NULL,
    landed      TEXT         NOT NULL,
    note        LONGTEXT     NULL,
    written_at  VARCHAR(48)  NOT NULL,
    PRIMARY KEY (bot_key, request_id, part),
    INDEX by_written_at (written_at)
);
