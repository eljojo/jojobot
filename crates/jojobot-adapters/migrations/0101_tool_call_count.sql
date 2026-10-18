-- How many calls reached each verb, by bot, day and outcome.
--
-- **A call is counted once, after its answer is known.** One row holds the
-- calls of one bot on one day to one verb that ended the same way. The usage
-- weeks read these rows; no other part of the server does.
--
-- `bot_key` is the permanent identity of the bot behind the call's session, as
-- a run's row keeps it, so a rename moves the handle and leaves the key. **A
-- call that carried no session belongs to no bot and is kept under the empty
-- key**, so it is counted and is not read as any bot's.
--
-- `day` is the day the run was in when the call was answered, `YYYY-MM-DD`: the
-- day the run stated, else the server's clock in the run's zone. It is never
-- read from the machine's own clock, so a run acting out another period is
-- counted in that period.
--
-- `verb` is the name of the verb and no argument or other text the caller sent.
-- `outcome` is `landed` or `refused`. `fix_by` is the word a refusal carried
-- (`retry`, `change` or `person`) and is empty for a call that landed and for a
-- refusal that carried none.
--
-- The primary key is the whole identity of a count, so two calls answered at the
-- same moment add to one row and neither is lost.
--
-- Creating a table is the one shape the runner can ask "is it there" about, so
-- this is one CREATE and nothing else.
CREATE TABLE tool_call_count (
    bot_key VARCHAR(191) NOT NULL,
    day     VARCHAR(10)  NOT NULL,
    verb    VARCHAR(48)  NOT NULL,
    outcome VARCHAR(8)   NOT NULL,
    fix_by  VARCHAR(8)   NOT NULL,
    calls   BIGINT       NOT NULL,
    PRIMARY KEY (bot_key, day, verb, outcome, fix_by)
);
