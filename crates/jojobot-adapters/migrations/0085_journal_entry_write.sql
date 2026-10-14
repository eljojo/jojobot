-- Every wording of a journal entry.
--
-- `journal_entry` keeps the text an entry holds NOW. A rewrite of the newest
-- entry, or of an automatic beat, overwrites the words before it. This keeps
-- each wording, one row each, in the order they were written, so an earlier one
-- can be read back. `journal_entry` stays the projection and the newest wording,
-- and answers every current read as before.
--
-- `ordinal` counts the wordings of ONE entry from one. `touched` is the moment
-- the entry was marked as moved, for a wording that moved it, as on the entry.
-- `written_at` is the moment on the server's own clock.
--
-- An entry written before this has no row here. Absence says nothing was
-- recorded, never that nothing happened, and nothing is backfilled.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE journal_entry_write (
    session    VARCHAR(64) NOT NULL,
    entry      VARCHAR(64) NOT NULL,
    ordinal    INT         NOT NULL,
    text       LONGTEXT    NOT NULL,
    touched    VARCHAR(48) NULL,
    written_at VARCHAR(40) NOT NULL,
    PRIMARY KEY (session, entry, ordinal)
);
