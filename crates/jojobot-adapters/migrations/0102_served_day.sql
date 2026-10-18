-- How many characters each run was served on each day.
--
-- **A run's served total is added to in the same transaction as the day it was
-- served on**, so the days of a run add up to the total the run was counted
-- over, and a run whose total was set back by a compaction keeps its days. The
-- usage weeks read these rows.
--
-- `session` is the id of the run (not its key and not its public name). The bot
-- is found through the run, so a bot renamed since is still one bot.
--
-- `day` is the day the run was in when the answer was served, `YYYY-MM-DD`: the
-- day the run stated, else the server's clock in the run's zone. It is never
-- read from the machine's own clock.
--
-- **No day before this table existed is filled in.** A run that was served
-- before has a total and no days; the usage week names the first day any answer
-- was counted on.
--
-- The primary key is the whole identity of a count. Creating a table is the one
-- shape the runner can ask "is it there" about, so this is one CREATE and nothing
-- else.
CREATE TABLE served_day (
    session VARCHAR(64) NOT NULL,
    day     VARCHAR(10) NOT NULL,
    chars   BIGINT      NOT NULL,
    PRIMARY KEY (session, day)
);
