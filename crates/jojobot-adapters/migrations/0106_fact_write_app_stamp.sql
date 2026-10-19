-- The app mark a claim carried at each write of it.
--
-- `fact.app_stamp` (0105) answers what a claim carries NOW. `fact_write` keeps
-- the whole claim on every write, so a read that projects the claim from its
-- writes finds the mark there too. The text is the one `fact.app_stamp` holds.
--
-- **Nullable and nothing backfilled**, for the reason 0105 is.
--
-- One statement with one clause, for the reason 0091 is.
ALTER TABLE fact_write ADD COLUMN app_stamp LONGTEXT NULL;
