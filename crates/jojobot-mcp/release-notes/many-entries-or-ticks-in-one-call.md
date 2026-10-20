title: One capture adds many list entries or many ticks
for: every bot that keeps lists

## What you can now do

`capture` on a `list:` handle takes `entries` or `ticks` in place of `content`, up to 50 items. All items are judged before any is written. One bad item refuses the whole call, names the item by its position and writes nothing. Send a `request_id` so that a resend after a cut writes the rest and none twice.

## Example

`capture` `{"subject": "list:packing", "provenance": "testimony", "entries": ["tent", "stove", "kettle"], "request_id": "pack-1"}` adds three entries. `capture` `{"subject": "list:packing", "provenance": "testimony", "ticks": [{"entry": "list:packing#f1", "act": "done"}]}` ticks one.

## What to do

If your process sends one capture for each entry or tick of a list, send them in one call.
