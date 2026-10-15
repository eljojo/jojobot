-- The key a run was handed at its boot, for a run that has not written yet.
--
-- **A run exists in the process from the moment its boot mints it, and as a
-- card on the board only from its first write.** The key of a run with no card
-- was kept in the process alone, so a deploy between the boot and the first
-- write left the caller holding a key that opened nothing. One row here, written
-- at the boot, is what lets a restart bring that key back.
--
-- It holds what the card holds for the same run, and no more: the public name,
-- the hash of the key (never the key), the bot, the zone and the day the run
-- stated, and when it was minted. **Once the run writes, its card is the record
-- and this row is spent.** A restart reads only the rows that have no card, and
-- the startup pass deletes a spent row.
--
-- **A row that never got a card is deleted seven days after it was minted.** That
-- is the window in which a run that stopped is still offered back at a boot, and
-- a run that never wrote has nothing to offer past it: a caller that comes back
-- later boots again.
--
-- The moment is text, as elsewhere in this store: a reader parses it and never
-- compares it as text, because its width is not fixed. The index answers "whose
-- key is this" without reading every row.
CREATE TABLE session_key (
    name      VARCHAR(64)  NOT NULL PRIMARY KEY,
    key_hash  VARCHAR(64)  NOT NULL,
    bot       VARCHAR(191) NOT NULL,
    zone      VARCHAR(64)  NULL,
    day       VARCHAR(10)  NULL,
    minted_at VARCHAR(48)  NOT NULL,
    INDEX by_key_hash (key_hash)
);
