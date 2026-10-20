title: A starred write names every rule that lost its seat
for: every bot that carries rules

## What you can now do

A boot carries a bot's starred rules up to its number of seats. When a starred `capture` or `update_fact` leaves rules with no seat, its receipt now carries `seats.dropped` as a list of every such rule's address, oldest first. It used to hold one address. The block is absent when no rule lost a seat.

## Example

Marge's bot has two seats. Marge captures a third starred rule with `capture` `{"subject": "bot:marge", "content": "check the receipt before moving on", "provenance": "testimony", "fields": {"starred": "true"}}`. The receipt holds `"seats": {"dropped": ["bot:marge#f1"]}`. A fourth starred rule gives `["bot:marge#f1", "bot:marge#f2"]`.

## What to do

If your process reads `seats.dropped` as one address, read it as a list. The rules in the list still stand and you can read them with `recall`, but a boot does not carry them whole.
