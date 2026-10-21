title: A tick that changes nothing is refused, and says which act lands
for: every bot that keeps lists

## What you can now do

A tick that would change nothing is refused: `undone` on an open entry, `done` on an entry that is already done, `rotated` on a list that is not a back list, and any act but `undone` on a dropped entry. The refusal names the act that lands instead, for a done entry on a back list that is `rotated`. In a `ticks` batch, one such item refuses the whole call and writes nothing, and the refusal names the item and says its reason once.

## Example

`capture` `{"subject": "list:homer-chores", "provenance": "testimony", "ticks": [{"entry": "list:homer-chores#f2", "act": "done"}, {"entry": "list:homer-chores#f2", "act": "done"}]}` is refused at item 2. Send `done`, `undone`, `done` in that order and all three ticks land.

## What to do

If your process sends the same tick twice, or ticks an entry to a state it is already in, send it once. If a call is refused, read the act the refusal names and send that.
