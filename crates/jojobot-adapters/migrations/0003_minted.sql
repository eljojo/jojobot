-- The id counter. `counter` rather than `next`, which this store's parser
-- treats as a reserved word.
CREATE TABLE minted (
    kind    VARCHAR(32) NOT NULL PRIMARY KEY,
    counter BIGINT      NOT NULL
);
