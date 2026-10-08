-- The newest charter change another identity made to a bot: who, and when.
--
-- One row per bot, rewritten in place by the next change. The question it
-- answers is whether a bot's charter changed since one of its sessions was last
-- handed it, and only the newest change can answer that. A bot rewriting its own
-- charter writes no row.
--
-- Kept in the store because a session outlives a restart: a change held only in
-- memory would be lost on the deploy that follows it.
CREATE TABLE charter_change (
    bot        VARCHAR(191) NOT NULL,
    changed_by VARCHAR(191) NOT NULL,
    changed_at VARCHAR(40)  NOT NULL,
    PRIMARY KEY (bot)
);
