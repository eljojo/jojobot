-- Every write of every entity row, in the order they happened: the cheap
-- signal a refresh checks before paying for a full read.
--
-- **This is a signal, not a substrate.** `fact_write` (0034) keeps the whole
-- claim so a correction can be told from a claim that always said the same
-- thing. Nothing here reads that far back for an entity: a rename, a prose
-- edit, a badge fill, an archive and a merge answer no such question today,
-- so this logs only that a write happened and when — enough to answer
-- "has anything changed since I last looked" without becoming a second
-- append-only history nothing reads.
--
-- `ordinal` counts the writes of ONE entity, from one, the same pattern
-- `fact_write` and `field_write` already use, so the current write is
-- whichever carries the highest ordinal without the reader depending on
-- row order.
CREATE TABLE entity_write (
    entity     VARCHAR(191) NOT NULL,
    ordinal    BIGINT       NOT NULL,
    written_at VARCHAR(40)  NOT NULL,
    PRIMARY KEY (entity, ordinal)
);
