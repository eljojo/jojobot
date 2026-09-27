-- The box that quarantined this message on purpose. NULL for every ordinary
-- row, and for every row quarantined by damage — a parse failure names no
-- box, because nobody decided anything.
ALTER TABLE message ADD COLUMN quarantined_by VARCHAR(191) NULL;
