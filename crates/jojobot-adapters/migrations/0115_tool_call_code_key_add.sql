-- Calls with different refusal reasons must remain separate counts.
ALTER TABLE tool_call_count
    ADD PRIMARY KEY (bot_key, day, verb, outcome, fix_by, code);
