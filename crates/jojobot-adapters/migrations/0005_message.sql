-- The messages in a mailbox.
--
-- Timestamps are text for the reason the session table's are: the domain's
-- instants are nanoseconds and DATETIME(6) is microseconds, so a column would
-- truncate a record that must read back as it was written.
--
-- `ordinal` is delivery order. A message's id is minted from a counter, so the
-- ids happen to sort, but a delivery that depended on that would break the day
-- the counter's shape changes; the order is a column instead.
--
-- `state` holds the token, not an enum. The set of states is the domain's and
-- a column that enforced it would be a second place the set is written down —
-- and a row wearing a token that is no state is the quarantine condition, which
-- the store must be able to HOLD in order to report it.
CREATE TABLE message (
    id          VARCHAR(64)  NOT NULL PRIMARY KEY,
    mailbox     VARCHAR(191) NOT NULL,
    ordinal     BIGINT       NOT NULL,
    body        LONGTEXT     NOT NULL,
    subject     TEXT         NULL,
    sender      VARCHAR(191) NOT NULL,
    sent_at     VARCHAR(48)  NOT NULL,
    state       VARCHAR(16)  NOT NULL,
    notes       TEXT         NULL,
    in_reply_to VARCHAR(64)  NULL,
    INDEX in_order (mailbox, ordinal)
);
