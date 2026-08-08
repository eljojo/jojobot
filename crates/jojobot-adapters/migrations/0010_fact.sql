-- One dated claim about one entity.
--
-- **`entity` and `id` are the address a caller edits through**, so they are the
-- key. One column, because whose claim it is and what it is about are the same
-- thing (rule 147): the two only ever came apart in a store where a row's page
-- could say something the row's own cell did not, and a column pair here would
-- carry that disagreement forward as though it were a concept.
--
-- **`standing` is NULL for a claim nobody declared one on.** The axis is
-- independent of provenance, and a standing that was never asserted is not the
-- same record as one that was: NULL says nobody said, and the reader derives
-- what the provenance implies. A column that stored the derived value would
-- lose the difference on the first read.
--
-- `date` is text for the reason every stamp here is text: the domain's date is
-- the domain's, and a column that reformatted it would rewrite a record on its
-- way through.
--
-- The edge is a shape and an object, both NULL together or both present. It is
-- one edge, not a set: the storage holds what the model holds.
--
-- `event_kind` is the whole of what an event needs in this row — its payload is
-- key-value and its references are handles, so both live in their own tables
-- where a query can reach them. A fact with no event has it NULL.
--
-- `derived_from` and `derived_from_id` are the address of the claim this one
-- came from, when it came from a claim rather than from an entity: the entity
-- and the local id, two columns rather than one string, so the pair is
-- addressable the way every other address here is.
CREATE TABLE fact (
    entity            VARCHAR(191) NOT NULL,
    id                VARCHAR(64)  NOT NULL,
    content           LONGTEXT     NOT NULL,
    details           LONGTEXT     NULL,
    provenance        VARCHAR(16)  NOT NULL,
    standing          VARCHAR(16)  NULL,
    status            VARCHAR(16)  NOT NULL,
    date              VARCHAR(16)  NOT NULL,
    edge_shape        VARCHAR(32)  NULL,
    edge_object       VARCHAR(191) NULL,
    event_kind        TEXT         NULL,
    derived_from      VARCHAR(191) NULL,
    derived_from_id   VARCHAR(64)  NULL,
    PRIMARY KEY (entity, id),
    INDEX by_edge (edge_object)
);
