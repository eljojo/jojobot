-- The window a wrap leaves open for one last change: the state and the code,
-- as one text. See WrapWindow in the domain.
--
-- NULL is a run that never wrapped, or whose window ended. Every row written
-- before this column reads as NULL, so no row is rewritten.
ALTER TABLE session ADD COLUMN wrap_window VARCHAR(80) NULL;
