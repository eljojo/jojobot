-- Every charter change, in the order it was made, whoever made it.
--
-- `charter_change` (0070) keeps the newest change another identity made to a
-- bot, one row per bot, rewritten in place, and a bot rewriting its own charter
-- writes no row there. This keeps every change, one row each, so an earlier
-- change and a bot's own rewrite can be read back. `charter_change` stays the
-- projection and answers every current read as before.
--
-- `ordinal` is the change's place among the changes to that bot, counted from 1.
-- `changed_by` is the identity that made the change, which is the bot itself for
-- its own rewrite. `changed_by_session` is the public name of the run that made
-- it, NULL when no run was known. `changed_at` is the moment on the server's
-- own clock. The charter text is not kept here.
--
-- A change made before this existed has no row here. Absence says nothing was
-- recorded, never that nothing happened, and nothing is backfilled.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE charter_change_write (
    bot                VARCHAR(191) NOT NULL,
    ordinal            INT          NOT NULL,
    changed_by         VARCHAR(191) NOT NULL,
    changed_by_session VARCHAR(64)  NULL,
    changed_at         VARCHAR(40)  NOT NULL,
    PRIMARY KEY (bot, ordinal)
);
