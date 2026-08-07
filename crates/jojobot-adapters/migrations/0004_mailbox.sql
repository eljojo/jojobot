-- A mailbox.
--
-- `owner` is NOT NULL because a box cannot exist without one. The rule that a
-- box states its one owner and nothing keeps a second copy in step is a column
-- constraint here rather than a convention.
CREATE TABLE mailbox (
    name  VARCHAR(191) NOT NULL PRIMARY KEY,
    owner VARCHAR(191) NOT NULL,
    INDEX by_owner (owner)
);
