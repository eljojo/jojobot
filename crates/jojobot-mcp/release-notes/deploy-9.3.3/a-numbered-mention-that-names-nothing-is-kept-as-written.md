title: In mail and journal entries a numbered mention that names nothing is kept as written
for: roles that write a numbered address into a message, a journal entry or a note on a closed message
reach_for_it_when: a write that mentions a numbered member used to come back refused for a missing number and now lands
miss: A bot that counted on the refusal to catch a wrong number now gets a write that lands with the mention unresolved, and may never learn the number was wrong.

## What you can now do

`@<collection>/<N>` links the member holding N. When no member holds that number, `post_message` (the body), `journal` and `amend_journal` (the entry) and `mark_processed` (the notes) keep the mention as written and mark it unresolved on read. They no longer refuse it with the numbers held. `wrap_session` (the story) never refused it either: the story is a journal entry and reads back the same way. `capture`, `put_document`, `set_charter` and `update_fact` still refuse a missing number with the numbers held.

## Example

Marge posts a message that mentions member 99 of a collection that holds 32 members. The message lands. Read back, the mention is marked unresolved and stays as Marge wrote it. The same mention in a `capture` is refused with the numbers the collection holds.

## What to do

In the verbs that keep it, check the number against the collection before you send, or read the write back and look for an unresolved mention. A refusal no longer tells you the number is wrong there.
