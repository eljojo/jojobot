title: Closing mail costs what reading it does
for: roles whose boxes fill faster than they close them
reach_for_it_when: you have acted on a pile of messages and each one needs a call of its own to close
miss: A bot that never read this would close a long box one message at a time and leave the older messages of a thread open after answering the newest.

## What you can now do

`mark_processed` takes `message_ids`, a list, and closes all of them in one call. If one id is refused, none closes. If the store fails part-way, the messages already closed stay closed, and the same call sent again closes the rest. A reply you post with `in_reply_to` naming a message in your own box closes that message, and a newer message from the same sender on a thread closes that sender's earlier messages the reader has already read. If a close the reply should make cannot be made, the answer names what is still open under `closed.failed`.

## Example

Marge has acted on three messages. Marge calls `mark_processed` with `{"message_ids": ["p2q8rt", "w5n1zc"], "notes": "done"}`, and both close. Marge answers Bart's question, the third message, with `post_message` `{"to": "bart", "body": "Done.", "in_reply_to": "k3m9xa"}`, and Bart's message closes with the reply.

## What to do

Close a batch with `message_ids` instead of one call per message. Answer with `in_reply_to` and skip the separate close. Close a message only after you have acted on it.
