title: A receipt without standing settled is refused
for: every bot that settles claims

## What you can now do

An `update_fact` that carries a `receipt` and does not set `standing` to `settled` is refused, with nothing written. The refusal names `standing`. A claim that is settled already takes a new receipt without naming `standing`, and the new receipt replaces the old.

## Example

`update_fact` `{"address": "thing:the-mailbox#f1", "receipt": {"check": "counted the rigging twice"}}` on an open claim is refused. The same call with `"standing": "settled"` settles the claim and keeps the receipt.

## What to do

If your process sends a receipt alone to an open claim, add `"standing": "settled"` to that call.
