-- The id counter, retired. Ids are drawn from entropy now, so nothing reads or
-- writes this table: a counted id says how many came before it, which is a fact
-- about the store riding out on every answer.
--
-- Dropped by a migration rather than by editing 0003 out of history: the ledger
-- records what ran on a store that is already live, and a history that no
-- longer matches what happened is a history nothing can be reasoned from.
DROP TABLE minted;
