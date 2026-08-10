-- How a message left `new`: one row per message that has been delivered.
--
-- **A row appears when a message is taken and never changes.** The first
-- delivery is the one recorded — a leftover was taken once already, and
-- rewriting it every time the recipient posted anything would destroy the very
-- signal this table exists to keep.
--
-- **A table rather than a column on `message`, and that is about the migration
-- runner rather than about the model.** A start decides whether an interrupted
-- migration landed by asking whether the object it creates is there. A column
-- addition reaches no such object: the runner would ask whether `message`
-- exists, find it, and record the version as applied whether or not the column
-- had ever landed — a schema and a ledger that disagree for ever, silently.
-- Creating a table is the one shape that question can answer.
--
-- A message with no row here has not been delivered, or was delivered before
-- there was anywhere to record how. Neither may be read as "nobody looked":
-- absence says nothing was recorded, never that nothing happened.
CREATE TABLE message_delivery (
    message_id VARCHAR(64) NOT NULL PRIMARY KEY,
    taken_by   VARCHAR(16) NOT NULL
);
