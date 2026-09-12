-- The claims a record has been marked as standing for.
--
-- Its own table rather than a cell of addresses, exactly as `fact_event_ref`
-- is: a mark is plural, and a question about what a record stands for is a
-- query here rather than a scan of every row.
--
-- Current state only, not versioned per write: a read answers what a record
-- stands for now, never what it stood for as of an old write. Setting the
-- mark again replaces this table's rows for the record whole; it does not
-- merge with what was there before the replace.
--
-- `ordinal` keeps the order a mark's sources were named in, so a read gives
-- them back as they were written; nothing reads meaning out of the order.
CREATE TABLE fact_stands_for (
    fact_home   VARCHAR(191) NOT NULL,
    fact_id     VARCHAR(64)  NOT NULL,
    ordinal     INT          NOT NULL,
    source_home VARCHAR(191) NOT NULL,
    source_id   VARCHAR(64)  NOT NULL,
    PRIMARY KEY (fact_home, fact_id, ordinal),
    INDEX by_source (source_home, source_id)
);
