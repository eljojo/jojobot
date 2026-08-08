-- An entity: a noun jojobot knows, addressed by its handle.
--
-- `id` is the handle and the primary key. It is identity rather than position
-- (rule 102), so nothing here renames it and no other column reconstructs it —
-- the kind is carried in the handle and stored beside it only so a filter is a
-- column read rather than a parse of every row.
--
-- `parent` holds the ONE edge of the tree, on the child, because upward is the
-- direction that is a single value. Children are derived by reading this column
-- the other way, so a parent and its child cannot come to disagree about who is
-- whose. It is deliberately NOT a foreign key: the store must be able to hold a
-- row whose parent was removed outside jojobot and report it, and a constraint
-- that refuses the row would turn a repairable record into an unreadable one.
--
-- `prose` is the human half of the page an entity used to be: everything that
-- was neither the machine block nor the fact table. It is a column rather than
-- a table because there is exactly one per entity and it is replaced whole.
CREATE TABLE entity (
    id     VARCHAR(191) NOT NULL PRIMARY KEY,
    kind   VARCHAR(32)  NOT NULL,
    name   TEXT         NOT NULL,
    source VARCHAR(191) NOT NULL,
    crm    VARCHAR(191) NULL,
    parent VARCHAR(191) NULL,
    boot   VARCHAR(16)  NOT NULL,
    prose  LONGTEXT     NOT NULL,
    INDEX by_kind (kind),
    INDEX by_parent (parent)
);
