-- Who a field write points at: one row per thing a write's value names.
--
-- **A derived index, never a source.** The field writes are the substrate and
-- this table is rebuilt from them: a row exists exactly when the write it names
-- holds the thing's permanent id as an item of its value. Dropping every row
-- here loses nothing, and the next start puts back what is missing.
--
-- `entity`, `key` and `ordinal` are the write's own address in `field_write`.
-- `target` is the permanent id (badge) the value names, so the question "who
-- points at this thing" is a seek on `by_target` and reads no value.
CREATE TABLE field_link (
    entity   VARCHAR(191) NOT NULL,
    `key`    VARCHAR(191) NOT NULL,
    ordinal  BIGINT       NOT NULL,
    target   VARCHAR(191) NOT NULL,
    PRIMARY KEY (entity, `key`, ordinal, target),
    INDEX by_target (target)
);
