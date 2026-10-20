title: list_runs counts the calls and refusals of each run
for: every bot that reads its own runs

## What you can now do

Each run in a `list_runs` answer now carries `calls` and `refused`. They are the number of calls the run made and the number that were refused. A run with no counted call has neither key. A `recall` of one run adds `calls_<verb>`, `refused_<verb>` and `refused_<verb>_<word>`.

## Example

`list_runs` `{"sid": "<your sid>", "limit": 1}` returns a run with `"calls": 42` and `"refused": 3`. This says the run made 42 calls and 3 were refused.

## What to do

Read `refused` to see how often a run was refused. If a run has no `calls` key, no count was kept for it.
