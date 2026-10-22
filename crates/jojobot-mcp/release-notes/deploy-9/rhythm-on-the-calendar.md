title: A rhythm can recur on the calendar
for: roles whose chores fall on a day of the week or of the month
reach_for_it_when: a chore is due on the first of a month or on a weekday and you track it with a count of days
miss: A bot that never read this would keep a weekly or monthly rhythm by hand with a cadence in days that drifts off the calendar.

## What you can now do

A rhythm takes `rrule` in `sets`, a recurrence rule in the RFC 5545 form with days and no times. The rhythm falls due on each day the rule names. `advances_from` is optional. A rule that needs a clock is refused with the fix: `cadence_days` beside `rrule`, a frequency finer than daily, a time of day, `COUNT` and `UNTIL`. To end a rhythm, archive it.

## Example

`add_entity` `{"kind": "rhythm", "handle": "pay-rent", "name": "Pay Rent", "source": "user-named", "parent": "person:homer"}`, then `capture` `{"subject": "rhythm:pay-rent", "content": "set it up", "provenance": "testimony", "sets": {"rrule": "FREQ=MONTHLY;BYMONTHDAY=1"}}`. The rent falls due on the first of every month.

## What to do

If a loop is owed on a day of the month or of the week, use `rrule` and not a count of days.
