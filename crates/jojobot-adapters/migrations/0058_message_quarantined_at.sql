-- When a deliberate quarantine landed. NULL for every ordinary row and for
-- one quarantined by damage, the same way quarantined_by is.
ALTER TABLE message ADD COLUMN quarantined_at VARCHAR(48) NULL;
