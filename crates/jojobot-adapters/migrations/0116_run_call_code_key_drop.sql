-- The next step restores uniqueness with the reason dimension. Both steps
-- finish before requests are served; a missing key witnesses this step alone.
ALTER TABLE run_call_count DROP PRIMARY KEY;
