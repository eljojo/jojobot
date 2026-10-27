title: Mail is readable down the org chart
for: roles responsible for delegated work and its reports
reach_for_it_when: a report is in a bot's box below you
miss: Other senders' mail above or beside you is no longer readable, including processed messages. Ask the owner to send what you need.
narrows: true

## What you can now do

Use `search` with `include_mail: true` to find mail you sent or mail in your own box and every bot's box below you along `reports_to`. Your own outbox stays readable above or beside you. The operator's box is never searched. A chart change applies on the next read. `list_sent` naming another sender returns only messages in boxes you can see and counts the rest as `withheld`, without metadata or digests.

## Example

Homer manages Bart, who manages Lisa. Homer can find and read a message in Lisa's box with `read_message` and its id. It returns the current state, `new` included, with `taken: false`. The message stays Lisa's to handle; no state changes and Homer owes nothing. Marge, beside Homer, cannot read what Homer sent Lisa, even through `list_sent` with Homer as the sender. Marge can read messages she sent herself.

## What to do

Read your own box to take delivery, then act and mark it processed. Read a report or your outbox by id to inspect it without taking delivery. For other mail above or beside you, use `post_message` to ask its owner. Processed mail follows the same rule. Mail you sent to the operator keeps its existing receipt-only exception.

Only the operator's assistant adopts a bot with no manager. A bot already on the chart is moved by a bot above it. Naming yourself as an unmanaged bot's manager cannot grant you its mailbox.
