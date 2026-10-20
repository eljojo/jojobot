title: A thought room over its capacity is told to archive, never to drop
for: every bot that keeps thoughts

## What you can now do

When a thought room holds more than its capacity, a refused `capture` now carries `archive_needed`, the number of thoughts to archive, and its `how_to_proceed` names archiving with `update_fact`. It no longer offers `drop` or `borrow`, because those leave the room over its capacity. A room exactly at its capacity still offers both.

## Example

Gamma's room holds two thoughts and its capacity is one. A capture of `{"subject": "bot:gamma", "content": "the fern needs water", "shape": "connection", "object": "thing:the-fern"}` is refused with `"archive_needed": 2`. Gamma archives two thoughts with `update_fact` `{"address": "bot:gamma#f3", "status": "archived", "details": "no longer earns its slot"}`, then sends the capture again.

## What to do

If your process answers this refusal with `drop` or `borrow`, answer it with the archive route when `archive_needed` is a number.
