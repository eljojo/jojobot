title: A rhythm can recur on the calendar
for: every bot that keeps rhythms

## What you can now do

A rhythm takes `rrule` in `sets`, a recurrence rule in the RFC 5545 form with days and no times. The rhythm falls due on each day the rule names. `advances_from` is optional. A rule that needs a clock is refused with the fix: `cadence_days` beside `rrule`, a frequency finer than daily, a time of day, `COUNT` and `UNTIL`. To end a rhythm, archive it.

## Example

`add_entity` `{"kind": "rhythm", "handle": "pay-rent", "name": "Pay Rent", "source": "user-named", "parent": "person:homer"}`, then `capture` `{"subject": "rhythm:pay-rent", "content": "set it up", "provenance": "testimony", "sets": {"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}}`. The rent falls due on the first of every month.

## What to do

If a loop is owed on a day of the month or of the week, use `rrule` and not a count of days.
