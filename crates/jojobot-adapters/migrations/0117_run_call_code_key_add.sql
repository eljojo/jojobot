-- A run retains the same refusal reasons as its bot's daily counts.
ALTER TABLE run_call_count
    ADD PRIMARY KEY (run, verb, outcome, fix_by, code);
