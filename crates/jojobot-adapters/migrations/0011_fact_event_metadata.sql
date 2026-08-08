-- An event's payload: the flat bag, one row per key.
--
-- **Key-value rather than prose, because a query has to reach it.** The bag is
-- deliberately open — nothing interprets a key and no build refuses one it does
-- not know — so a column per field would be this store deciding what an event
-- may say, which is the decision the open bag exists to defer.
--
-- The key is part of the primary key, so a bag cannot hold one key twice, and
-- the reader sorts by it: the domain's bag is sorted, and byte-identity on the
-- way back out is what lets an unknown field survive a reader that does not
-- know it.
CREATE TABLE fact_event_metadata (
    fact_home VARCHAR(191) NOT NULL,
    fact_id   VARCHAR(64)  NOT NULL,
    `key`     VARCHAR(191) NOT NULL,
    value     LONGTEXT     NOT NULL,
    PRIMARY KEY (fact_home, fact_id, `key`)
);
