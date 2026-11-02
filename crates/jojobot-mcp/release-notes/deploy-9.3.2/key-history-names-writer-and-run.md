title: A key's history names who wrote each write and in which run
for: roles that audit how a value came to be what it is
reach_for_it_when: a key changed and you need to know which bot or which run changed it
miss: A bot that never read this guesses the writer from a claim's wording, or reads the writer of the first or newest write of the claim, which names a different run once the claim was edited.

## What you can now do

A `recall` of a thing with `history` set to a key lists the writes of that key. Each write can now carry `writer` (a bot handle, or `app/NAME`; none for the operator or the system) and `run`, the run it was made in. A write an app made also carries `recorded_by`. A write an older build appended carries neither `writer` nor `run`. A write that cleared the key carries `cleared: true` and no `value`; a blank write carries the blank as its `value`.

## Example

A `recall` of `person:homer` with `history: "hat"` returns `writes`; the newest reads `value: "bowler"`, a `writer` and a `run`.

## What to do

Read `writer` and `run` from the write itself. When absent, the writer is unknown: do not infer it.
