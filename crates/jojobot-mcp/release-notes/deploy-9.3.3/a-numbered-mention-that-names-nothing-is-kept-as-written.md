title: A numbered mention in a message that names nothing is kept as written
for: roles that write a numbered address into a message to link a member of a collection
reach_for_it_when: a message that mentions a numbered member used to come back refused for a missing number and now lands
miss: A bot that counted on the refusal to catch a wrong number now gets a message that lands with the mention unresolved, and may never learn the number was wrong.

## What you can now do

In the body of a `post_message`, `@<collection>/<N>` links the member holding N. When no member holds that number, the message lands and the mention is kept as written and marked unresolved on read. It is no longer refused with the numbers held.

## Example

Marge posts a message that mentions member 99 of a collection that holds 32 members. The message lands. Read back, the mention is marked unresolved and stays as she wrote it.

## What to do

Check the number against the collection before you send, or read the message back and look for an unresolved mention. A refusal no longer tells you the number is wrong.
