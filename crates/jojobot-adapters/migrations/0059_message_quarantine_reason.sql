-- Why a deliberate quarantine was made, in the quarantining bot's own words.
-- NULL for every ordinary row and for one quarantined by damage, the same
-- way quarantined_by and quarantined_at are.
ALTER TABLE message ADD COLUMN quarantine_reason TEXT NULL;
