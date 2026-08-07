-- The record of the one-time handover.
--
-- Without it, done-ness is inferred from "the target has rows in it", which
-- cannot tell a completed run from a half-verified one — and once this store is
-- authoritative that inference is worse than useless: wiping the data directory
-- is the obvious repair, the old store still holds the pre-migration snapshot,
-- and an empty target then looks exactly like a fresh install. The next start
-- would carry the OLD records back over everything written since. This row is
-- what makes those two states different.
--
-- **Two states, because verification is post-commit.** `written` goes in with
-- the carried rows, so a death before that commit leaves no row and no data;
-- `verified` is set once the read-back passed, and only that one means the
-- store may be served from. Same shape as the migration runner's begun marker,
-- for the same reason.
--
-- `what` is the primary key a one-row table needs, and it makes the row say
-- what it is a record OF rather than being a bare flag.
CREATE TABLE handover (
    what  VARCHAR(64) NOT NULL PRIMARY KEY,
    state VARCHAR(16) NOT NULL
);
