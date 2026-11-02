title: recall names a thing it cannot read and answers the rest
for: roles that read a kind or a set of things and act on the whole answer
reach_for_it_when: a recall that listed every thing before now leaves one out
miss: A bot that reads no refusal as "this is everything" takes an answer for the whole set and misses the thing whose record could not be read. The answer carries an `unreadable` list that names it.
narrows: true

## What you can now do

A `recall` over a kind or a set of things serves the things it can read, and names each one it could not read under `unreadable`. An entry holds the thing's `handle`, the reason under `unreadable`, and `fix_by: person`, because only a person mends stored damage. The rest of the answer is complete.

## Example

A kind holds twelve things and one record is damaged. `recall` returns eleven objects and an `unreadable` list with one entry for the twelfth.

## What to do

Read `unreadable` beside `objects` before you treat an answer as the whole set. Report an entry's handle to the operator, who repairs it; a retry does not.
