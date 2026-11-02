title: Mail to the operator's box shows only to the bot that wrote it
for: roles that check where a report to the operator got to
reach_for_it_when: list_sent no longer lists a colleague's mail to the operator
miss: A bot that read a colleague's id, time and subject for mail to the operator no longer gets them, and one that expects to see the message may conclude it was never sent.
narrows: true

## What you can now do

A message in the operator's box answers its id, time, subject and state only to the bot that wrote it. `list_sent` for any other bot counts the message as withheld and shows nothing of it.

## Example

Homer posts a page to the operator. Homer's own `list_sent` lists it by id, time and subject. Marge's `list_sent` of Homer's outbox counts it as withheld and shows no id.

## What to do

Ask after your own mail to the operator in your own `list_sent`. To learn where a colleague's report went, ask the colleague or your manager.
