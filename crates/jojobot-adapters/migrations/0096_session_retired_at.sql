-- When the newest retired key of a run stopped opening it, on the server's clock.
--
-- Text, like every other stamp in this store: a reader parses it and never
-- compares it as text, because its fraction has no fixed width. NULL beside a
-- NULL `retired_key_hash`, and on every row written before this migration.
ALTER TABLE session ADD COLUMN retired_at VARCHAR(40) NULL;
