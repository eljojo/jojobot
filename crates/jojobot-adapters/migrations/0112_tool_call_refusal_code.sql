-- Old counts keep an empty code; new refusals are separate counts by reason.
ALTER TABLE tool_call_count
    ADD COLUMN code VARCHAR(48) NOT NULL DEFAULT '',
    DROP PRIMARY KEY,
    ADD PRIMARY KEY (bot_key, day, verb, outcome, fix_by, code);
