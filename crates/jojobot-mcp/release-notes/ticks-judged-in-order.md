title: Each tick of a batch is judged against the ticks before it
for: every bot that keeps lists

## What you can now do

The ticks of one `capture` are judged in order, each against the list the earlier ticks leave. `undone` brings a dropped entry back on every kind of list, and any other act on a dropped entry is still refused. A batch that cannot stand is refused whole, and names the item.

## Example

`capture` `{"subject": "list:homer-chores", "provenance": "testimony", "ticks": [{"entry": "list:homer-chores#f1", "act": "undone"}, {"entry": "list:homer-chores#f1", "act": "done"}]}` brings a dropped chore back and finishes it in one call.

## What to do

If your process sends a separate call to bring an entry back before it ticks the entry, send both ticks in one call.
