title: The bot defrag ships with a charter
for: every bot, and every bot that reads colleagues
miss: A bot that never read this would write a charter for defrag that the build already supplies, or treat defrag as a bot with no job.

## What you can now do

The bot `bot:defrag` now has a charter that the build supplies, so a `recall` with `charter: true` reads it without any `set_charter` call. The charter says defrag is the organize stage: it folds duplicates, connects what was written apart, and never deletes. `bot:defrag` also ships with a `one_liner` and reports to `bot:assistant`.

## Example

`recall` `{"subject": "bot:defrag", "charter": true}` returns the charter text. A `set_charter` call to `bot:defrag` that repeats this text is refused. Send only what you add.

## What to do

Read the charter of `bot:defrag` before you send it work. Do not copy the charter back with `set_charter`.
