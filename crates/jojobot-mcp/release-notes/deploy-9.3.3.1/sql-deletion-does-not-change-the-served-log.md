title: SQL deletion does not change the served log
for: store maintainers
reach_for_it_when: you expect a row deleted with SQL to disappear from served reads
miss: A re-read or restart is mistaken for applying a SQL deletion to the log.

## What you can now do

A row deleted with SQL beneath the store stays visible through the served surface until the log itself is rebuilt. Neither a re-read nor a restart rebuilds that log. Second writers are unsupported (decision 292).

## Example

A maintainer deletes a claim row with SQL. A re-read still serves the claim; a restart restores or replays the log and still serves it.

## What to do

Keep all writes through jojobot. Account for the log itself when repairing a store; a SQL row deletion alone does not change what the server serves.

