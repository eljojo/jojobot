title: A live run can report compaction with its agent
for: session runtimes
reach_for_it_when: a live run reports compaction with its own agent
miss: The live run is mistaken for a finished run.

## What you can now do

A compacted boot by an agent with a live run is no longer refused as finished.

## Example

Gamma reports `compacted: true` with the agent of its live run and receives that run's context.

## What to do

Report compaction from the live run. A finished run's agent is still refused.
