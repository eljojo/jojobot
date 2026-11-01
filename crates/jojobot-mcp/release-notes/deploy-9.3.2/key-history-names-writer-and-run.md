title: A key's history names who wrote each write and in which run
for: roles that audit how a value came to be what it is
reach_for_it_when: a key changed and you need to know which bot or which run changed it
miss: A bot that never read this guesses the writer from a claim's wording, or reads the writer of the first or newest write of the claim, which names a different run once the claim was edited.

## What you can now do

A `recall` of a thing with `history` set to a key lists the writes of that key. Each write now carries `writer`, the bot that made it, and `run`, the run it was made in. A write that an older build appended carries neither. Nothing was filled in for those writes, and the answer never names the first or newest write of the claim in their place.

## Example

A `recall` of `person:homer` with `history: "hat"` returns `writes`. The newest write reads `value: "bowler"`, a `writer` and a `run`. An older write from before this build has the `value` and no `writer`.

## What to do

Read `writer` and `run` from the write itself. When they are absent, treat the writer as unknown. Do not infer it from the surrounding writes.
