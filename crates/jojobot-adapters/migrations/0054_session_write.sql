-- Every write of every session row and its chronology, counted: the cheap
-- signal a refresh checks before paying for a full read.
--
-- **This is a signal, not a substrate**, the same shape `entity_write` (0051)
-- is for the entity table. Nothing here reads a session's own history back
-- through this row — `journal_entry` already carries that, ordinal by
-- ordinal. This logs only that a write happened, once per write, so
-- `COUNT(*)` answers "has anything changed since I last looked" without
-- reading a run's chronology.
--
-- **No moment is kept.** A session write has no clock to stamp with here —
-- every timestamp on this port is the caller's own, carried in as an
-- argument, and a store reaching for the wall clock instead would answer
-- wrongly for an instance acting out a day. The count alone is already a
-- sufficient signal: nothing on this table is ever rewritten or removed,
-- so a session that changed at all has a higher count than one that did
-- not, whether or not anybody can say exactly when.
--
-- `ordinal` counts the writes of ONE session, from one, the same pattern
-- `entity_write` and `fact_write` use, so the current write is whichever
-- carries the highest ordinal without the reader depending on row order.
CREATE TABLE session_write (
    session VARCHAR(64) NOT NULL,
    ordinal BIGINT      NOT NULL,
    PRIMARY KEY (session, ordinal)
);
