-- The same column on the substrate, because every write carries the whole
-- claim — see 0038_fact_write_happened_at for the reason a write table
-- without this could not say what a write said about the span.
--
-- **Nullable and unfilled**, for the reason the claim's own column is.
ALTER TABLE fact_write ADD COLUMN happened_through VARCHAR(16) NULL;
