-- The index answers "which message replaced this one" without reading every
-- row. The column it covers is added by the migration before this one,
-- 0076_message_supersedes, which carries the account of the pair.
ALTER TABLE message ADD INDEX by_supersedes (supersedes);
