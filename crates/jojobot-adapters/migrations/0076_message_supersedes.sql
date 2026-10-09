-- The message a message replaces, stored on the NEW message only. NULL for every
-- row written before this column existed, and for every message that replaces
-- nothing. What the old message reads as `superseded_by` is worked out from
-- this column when it is read, so there is no second copy to keep in step.
--
-- The index answers "which message replaced this one" without reading every
-- row. It is added in the same statement, because this store commits DDL as it
-- goes and a second statement could be left unapplied.
ALTER TABLE message
    ADD COLUMN supersedes VARCHAR(64) NULL,
    ADD INDEX by_supersedes (supersedes);
