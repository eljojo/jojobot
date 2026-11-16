title: Snapshots follow event count and shape changes log INFO
for: restart and memory maintainers
reach_for_it_when: you estimate replay after a kill or read a shape-change boot log
miss: A quiet five-minute interval is mistaken for a snapshot boundary.

## What you can now do

Snapshot cadence is event-count only: 10,000 events. The five-minute dirty trigger is gone. After a kill, the events written since the last snapshot replay, normally fewer than 10,000. The threshold is checked every 30 seconds, so 10,000 is not a replay limit. A snapshot shape change logs INFO instead of WARN.

## Example

A run writes fewer than 10,000 events and stays quiet for five minutes. That wait does not trigger a snapshot.

## What to do

Plan replay by event count. Treat an INFO shape change as an expected replay on the new build.
