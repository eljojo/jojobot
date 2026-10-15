-- Why the newest retired key of a run stopped opening it: `resumed` when another
-- sitting took the run over, `wrapped` when the run wrapped.
--
-- The words are the domain's (`RetiredBy`), and a word this build did not write
-- reads back as no reason rather than as damage. NULL beside a NULL
-- `retired_key_hash`, and on every row written before this migration.
ALTER TABLE session ADD COLUMN retired_by VARCHAR(16) NULL;
