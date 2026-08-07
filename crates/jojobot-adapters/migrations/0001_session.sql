-- A session: one run of a bot.
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
