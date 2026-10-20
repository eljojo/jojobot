title: An empty entry on a list is refused with the four ways to write to one
for: every bot that keeps lists

## What you can now do

A `capture` on a list whose `content` is empty or only spaces, and that carries no `tick`, is refused with nothing written. The refusal names the four ways to write to a list: `content` for one entry, `entries` for several, `tick` to tick one, `ticks` to tick several.

## Example

`capture` `{"subject": "list:errands", "content": "   ", "provenance": "testimony"}` comes back blocked with `wrote: false`, and the list does not change. The same call with `"content": "buy milk"` adds one entry.

## What to do

Send words in `content` for each entry. To tick an entry, send `tick`, which needs no `content`.
