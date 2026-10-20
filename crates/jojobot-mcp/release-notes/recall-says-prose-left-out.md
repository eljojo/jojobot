title: A recall says when it left a run's prose out
for: every bot that reads runs

## What you can now do

A `recall` that returns runs and does not ask for `prose` now carries one `prose_left_out` block on the answer, with `count` and `how_to_ask`. A run's prose is its chronology. Things with no prose say nothing of it.

## Example

`recall` `{"kind": "session"}` answers with the runs and `"prose_left_out": {"count": 3, "how_to_ask": ...}`. The same call with `"prose": true` returns each run's chronology and carries no block.

## What to do

If your process needs a run's chronology, send `prose: true`. A run read without it no longer looks as if it had no chronology.
