-- Every write of every key, in the order they happened: the substrate a field
-- read projects from.
--
-- **The address is the thing and the key.** `entity` and `key` are what a
-- history is asked for, and what the current value is decided over — never the
-- record's id. A hundred sittings that each record one donut are a hundred
-- records, so rows keyed by the record would be a hundred histories of length
-- one, and nothing could be counted from them.
--
-- `ordinal` counts the writes of ONE key on ONE thing, from one. It is what
-- makes the newest write the current value and the oldest write the first
-- entry, without the reader depending on the order rows came back in.
--
-- **`value` is NULL when the write took the key off.** A clear is a write like
-- any other: nothing is removed here, so the key stops being current while the
-- writes that put it there stay readable. NULL and the empty string are
-- different rows, because a caller may write an empty value and mean it.
--
-- `fact_id` is the record that carried the write, and it is a column rather
-- than part of the key: a record projects its own fields from the newest write
-- each of its keys made, and the index is what makes that read cheap.
CREATE TABLE field_write (
    entity   VARCHAR(191) NOT NULL,
    `key`    VARCHAR(191) NOT NULL,
    ordinal  BIGINT       NOT NULL,
    value    LONGTEXT     NULL,
    fact_id  VARCHAR(64)  NOT NULL,
    PRIMARY KEY (entity, `key`, ordinal),
    INDEX by_record (entity, fact_id)
);
