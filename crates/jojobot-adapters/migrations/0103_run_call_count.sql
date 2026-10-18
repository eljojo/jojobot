-- How many calls each run made to each verb, and how they ended.
--
-- **A call is counted once, in the same step as its count under the bot and the
-- day** (`tool_call_count`, 0101), so the two agree. One row holds the calls of
-- one run to one verb that ended the same way.
--
-- `run` is the run's public name, never the card id and never the key. A run
-- has a public name from its boot, and a card only from its first write, so a
-- call made before the first write is kept. A call that carried no session
-- belongs to no run and has no row here.
--
-- `verb`, `outcome` and `fix_by` are what `tool_call_count` keeps: the name of
-- the verb and no argument or other text the caller sent, `landed` or `refused`,
-- and the word a refusal carried (`retry`, `change` or `person`) or empty.
--
-- **No call before this table existed is filled in**: the old counts hold no run.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE run_call_count (
    run     VARCHAR(64) NOT NULL,
    verb    VARCHAR(48) NOT NULL,
    outcome VARCHAR(8)  NOT NULL,
    fix_by  VARCHAR(8)  NOT NULL,
    calls   BIGINT      NOT NULL,
    PRIMARY KEY (run, verb, outcome, fix_by)
);
