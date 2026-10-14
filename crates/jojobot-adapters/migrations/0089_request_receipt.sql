-- The request ids of writes that landed, one row each, and the receipt each
-- one returned.
--
-- A caller may send a request id with a write. The row is inserted in the SAME
-- transaction as the write, so a write and its row land together or not at all.
-- The primary key makes two calls with one id from one bot race to one winner:
-- the second insert fails and its whole write rolls back.
--
-- `bot_key` is the permanent identity of the bot that sent the id, never its
-- handle, so a rename cannot hand one bot's receipt to another. The same id from
-- another bot is a different row.
--
-- `verb` is the verb that wrote. `args_digest` is a digest of the arguments the
-- first call sent, so a repeat can say whether its arguments differed.
-- `landed` holds what the store returned for the write (the ids it wrote) as
-- JSON, kept in the transaction so a repeat can answer from it when the receipt
-- is missing. `receipt` holds the answer the verb gave, filled in after the
-- transaction commits, and is NULL when the process stopped between the two.
--
-- `written_at` is the moment on the server's own clock. Rows older than the
-- retention period are removed by the adapter.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE request_receipt (
    bot_key     VARCHAR(191) NOT NULL,
    request_id  VARCHAR(128) NOT NULL,
    verb        VARCHAR(48)  NOT NULL,
    args_digest VARCHAR(64)  NOT NULL,
    landed      TEXT         NOT NULL,
    receipt     LONGTEXT     NULL,
    written_at  VARCHAR(48)  NOT NULL,
    PRIMARY KEY (bot_key, request_id),
    INDEX by_written_at (written_at)
);
