-- What a claim pointed at and stood for at each write of it.
--
-- `fact_event_ref` (0012) and `fact_stands_for` (0044) answer what a claim
-- points at and stands for NOW: a write replaces their rows whole, so a mark set
-- twice leaves nothing of the first. `fact_write` (0034) keeps the whole claim on
-- every write, and these two columns keep the rest of it, so an earlier set can be
-- read back. Both tables stay as the projection and answer every current read as
-- before.
--
-- `refs` is the entities the claim pointed at, as one text: a JSON array of the
-- keys the entities are stored under, in the order they were named. `stands_for`
-- is the claims it stood for: a JSON array of [home key, claim id] pairs, in
-- order. A key is the permanent id of the thing, so a read after a rename serves
-- the handle the thing wears now.
--
-- **Every column is nullable and nothing is backfilled.** A row an older build
-- appended holds neither, the earlier sets are gone, and it stays NULL. A reader
-- treats NULL as "not recorded", never as a write that pointed at nothing.
--
-- One statement, because this store commits DDL as it goes and a second
-- statement could be left unapplied.
ALTER TABLE fact_write
    ADD COLUMN refs       TEXT NULL,
    ADD COLUMN stands_for TEXT NULL;
