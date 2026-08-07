-- The chronology under a session.
--
-- `ordinal` is the chronology's order and the whole of the append-only rule:
-- the newest entry is MAX(ordinal), and everything below it is history.
CREATE TABLE journal_entry (
    session  VARCHAR(64)  NOT NULL,
    id       VARCHAR(64)  NOT NULL,
    ordinal  INT          NOT NULL,
    at       VARCHAR(48)  NOT NULL,
    text     LONGTEXT     NOT NULL,
    touched  VARCHAR(48)  NULL,
    beat     VARCHAR(191) NULL,
    PRIMARY KEY (session, id),
    INDEX in_order (session, ordinal)
);
