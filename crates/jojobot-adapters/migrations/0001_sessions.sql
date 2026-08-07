-- A session, and the chronology under it.
--
-- Timestamps are text, deliberately: DATETIME(6) holds microseconds and the
-- domain's instants are nanoseconds, so a column would truncate a record that
-- must read back as it was written. The sweep compares instants.
CREATE TABLE session (
    id          VARCHAR(64)  NOT NULL PRIMARY KEY,
    sid         VARCHAR(64)  NULL,
    bot         VARCHAR(191) NOT NULL,
    focus       TEXT         NOT NULL,
    started_at  VARCHAR(48)  NOT NULL,
    state       VARCHAR(16)  NOT NULL,
    INDEX by_bot (bot)
);

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

-- The id counter. `counter` rather than `next`, which this store's parser
-- treats as a reserved word.
CREATE TABLE minted (
    kind    VARCHAR(32) NOT NULL PRIMARY KEY,
    counter BIGINT      NOT NULL
);
