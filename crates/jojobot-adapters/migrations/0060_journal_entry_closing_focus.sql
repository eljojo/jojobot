-- The run's own focus, exactly as it stood when this entry closed the
-- session. Only wrap_session ever writes it.
--
-- NULL for every ordinary entry and for every row written before this
-- column existed.
ALTER TABLE journal_entry ADD COLUMN closing_focus TEXT NULL;
