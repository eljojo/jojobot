-- Every write of every claim, in the order they happened: the substrate a
-- claim read projects from.
--
-- **The claim's own row is the projection and this is what it projects over.**
-- A claim was a row rewritten in place, so a correction overwrote what it used
-- to say and nothing anywhere remembered. A session could not tell "we never
-- recorded this" from "we recorded it and we were wrong".
--
-- **The address is the thing and the RECORD**, where the field substrate's is
-- the thing and the KEY. That difference is not an inconsistency: a key's
-- writes are counted across every record that touched it, and a claim's writes
-- belong to one claim and are counted for nobody. Each substrate is addressed
-- by what its own question is asked of.
--
-- `ordinal` counts the writes of ONE claim, from one, so the newest write is
-- the claim as it now stands and the oldest is what it first said — without
-- the reader depending on the order rows came back in.
--
-- **A write carries the WHOLE claim.** Content and edge alone would not do:
-- the fold that projects a thing's fields keeps only writes whose record is
-- ACTIVE, so a status left in a column above a projected content is a filter
-- reading a value it is supposed to be deciding. Status is versioned with
-- everything else or none of it is.
--
-- **Nothing reads this yet.** It is written beside the claim's own row and the
-- reads still come from that row, so this migration and the fill behind it
-- change no answer.
CREATE TABLE fact_write (
    entity            VARCHAR(191) NOT NULL,
    fact_id           VARCHAR(64)  NOT NULL,
    ordinal           BIGINT       NOT NULL,
    content           LONGTEXT     NOT NULL,
    details           LONGTEXT     NULL,
    provenance        VARCHAR(16)  NOT NULL,
    standing          VARCHAR(16)  NULL,
    status            VARCHAR(16)  NOT NULL,
    date              VARCHAR(16)  NOT NULL,
    edge_shape        VARCHAR(32)  NULL,
    edge_object       VARCHAR(191) NULL,
    derived_from      VARCHAR(191) NULL,
    derived_from_id   VARCHAR(64)  NULL,
    inserted_at       VARCHAR(40)  NULL,
    stale_after       VARCHAR(16)  NULL,
    PRIMARY KEY (entity, fact_id, ordinal)
);
