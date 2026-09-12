-- The same backfill as `0046_fact_status_archived`, over `fact_write`: every
-- write carries its own copy of the status the record held at that write, so
-- the retired tokens are on this table too and need the same rewrite.
UPDATE fact_write SET status = 'archived' WHERE status IN ('superseded', 'retracted', 'negated');
