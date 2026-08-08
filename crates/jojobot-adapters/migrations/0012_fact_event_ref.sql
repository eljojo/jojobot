-- The entities an event points at.
--
-- Their own rows rather than a cell of handles, because they are references: a
-- question about what points at an entity is a query here and a scan of every
-- payload otherwise.
--
-- `ordinal` keeps the order they were written in, so a record reads back as it
-- was written; nothing reads meaning out of the order.
CREATE TABLE fact_event_ref (
    fact_home VARCHAR(191) NOT NULL,
    fact_id   VARCHAR(64)  NOT NULL,
    ordinal   INT          NOT NULL,
    entity    VARCHAR(191) NOT NULL,
    PRIMARY KEY (fact_home, fact_id, ordinal),
    INDEX by_entity (entity)
);
