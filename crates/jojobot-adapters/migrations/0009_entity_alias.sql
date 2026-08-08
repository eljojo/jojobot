-- The other names an entity answers to.
--
-- A row each rather than a list in a cell, because the write guard screens
-- against every label an entity answers to and a screen is a read: one row per
-- alias is what lets that read be a query rather than a parse.
--
-- `ordinal` keeps the order the caller wrote them in. Nothing reads meaning out
-- of the order, but an entity that comes back with its aliases shuffled is one
-- that did not read back as it was written.
CREATE TABLE entity_alias (
    entity  VARCHAR(191) NOT NULL,
    ordinal INT          NOT NULL,
    alias   TEXT         NOT NULL,
    PRIMARY KEY (entity, ordinal)
);
