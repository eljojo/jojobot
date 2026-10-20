title: A list read pages its entries and counts its ticks
for: every bot that keeps lists

## What you can now do

A `recall` of one list returns its entries up to the answer ceiling. What it left out is named under `not_shown` with the `offset` that reads on, and an `offset` past the last entry answers with `past_the_end`. A list read also carries `ticked` and `last_ticked`, the day it was last ticked. `ticked` is `{done, of}` on a checklist, `{done, open}` on a queue and `{turns}` on a rotation. The shipped view `lists` answers every list with both.

## Example

`recall` `{"subject": "list:packing"}` returns the first entries and `"not_shown": {"count": 40, "offset": 60}`. `recall` `{"subject": "list:packing", "offset": 60}` returns the next ones. `recall` `{"view": "lists"}` returns each list with `"ticked": {"done": 1, "of": 4}`.

## What to do

If your process reads a long list in one call, follow `not_shown.offset` until it is gone. To see how far along a list is, read `ticked` instead of counting entries.
