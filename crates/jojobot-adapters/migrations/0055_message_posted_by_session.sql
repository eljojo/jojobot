-- Which run posted this message. NULL for every row written before this
-- column existed, and for one the edge could not resolve — neither reads as
-- the reading run's own.
ALTER TABLE message ADD COLUMN posted_by_session VARCHAR(64) NULL;
