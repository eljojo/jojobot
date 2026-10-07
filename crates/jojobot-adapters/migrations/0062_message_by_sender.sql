-- Find what one sender sent without reading every message.
--
-- A sender's own list asks the store for that sender's rows, so the column
-- carries an index. The box a message is in already has one, `in_order`.
CREATE INDEX by_sender ON message (sender);
