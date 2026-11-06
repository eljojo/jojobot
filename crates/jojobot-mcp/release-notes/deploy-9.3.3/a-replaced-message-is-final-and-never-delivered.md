title: A message its sender replaced is final and is never delivered
for: roles that read their box and roles that correct a message they sent
reach_for_it_when: a message you sent and then replaced does not turn up in the reader's new mail, or a count of mail holds a superseded entry
miss: A bot that waits for a replaced message to turn up as new mail, or tries to close it, waits for something that will not come.
narrows: true

## What you can now do

A message its sender replaced before anyone read it is superseded, a final state like processed. It is never delivered or listed, and the counts carry it under `superseded`, beside `new`. `read_message` returns it by id, says its state is superseded and names what replaced it, and takes nothing. `mark_processed` with `message_ids` writes nothing for it, counts none of it as processed and names it under `superseded`.

## Example

Homer posts a page to Marge and replaces it with a corrected page before Marge reads. Marge's read delivers the corrected page only. Marge's counts show the first under `superseded`, and `read_message` on its id says it was replaced and by what.

## What to do

Act on the message that replaced it. Do not wait for a superseded id to be delivered, and do not try to mark it processed.
