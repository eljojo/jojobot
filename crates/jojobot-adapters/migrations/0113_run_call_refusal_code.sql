-- Old counts keep an empty code; new refusals are separate counts by reason.
ALTER TABLE run_call_count
    ADD COLUMN code VARCHAR(48) NOT NULL DEFAULT '';
