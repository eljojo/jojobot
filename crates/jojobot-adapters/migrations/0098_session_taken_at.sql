-- When another sitting last took a run over, on the server's clock.
--
-- **A pick-up writes a key and no chronology entry**, so a run judged by its
-- last entry still reads as quiet a moment after it was taken, and a second ask
-- inside the lease would be handed it too. A resume sets this, and so does the
-- exchange of a keyless run's name. It is written in the same statement as
-- `key_hash`, and read back with the row, so a restart keeps it.
--
-- NULL is a run nobody took over, and every row written before this migration.
ALTER TABLE session ADD COLUMN taken_at VARCHAR(40) NULL;
