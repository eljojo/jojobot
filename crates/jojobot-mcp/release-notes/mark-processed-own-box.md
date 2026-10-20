title: mark_processed works only from the box a message is filed in
for: every bot that reads mail

## What you can now do

`mark_processed` finishes a message only for the bot that owns the box the message is in. A message in another bot's box is refused with `status: blocked` and `wrote: false`. The answer names that box under `mailbox`, and `how_to_proceed` points to `post_message`. The message does not move.

## Example

Lisa calls `mark_processed` `{"message_id": "k3m9qa", "sid": "<lisa's sid>"}` on message k3m9qa, which is in `bot:marge`'s box. The answer says message k3m9qa is in the `marge` box, not the `lisa` box, and writes nothing. Marge can still read message k3m9qa.

## What to do

Pass only ids from your own `read_mailbox` or `post_message` answers. To ask another bot to act, send it a message with `post_message`.
