title: recall names a thing it cannot read and answers the rest
for: roles that read a kind or a set of things and act on the whole answer
reach_for_it_when: a recall that listed every thing before now leaves one out
miss: A bot that reads no refusal as "this is everything" takes an answer for the whole set and misses the thing whose record could not be read. The answer carries an `unreadable` list that names it.
narrows: true

## What you can now do

A `recall` over a kind or a set of things serves the things it can read, and names each one it could not read under `unreadable`. An entry holds the thing's `handle`, the reason it could not be read under `unreadable`, and `fix_by: person`, because stored damage is mended by a person and no call a session makes mends it. The rest of the answer is complete without those things. Before, one damaged record could refuse the whole read.

## Example

Homer's page of a kind holds twelve things, and one record is damaged. `recall` returns eleven objects and an `unreadable` list with one entry, the twelfth thing's `handle`, the reason, and `fix_by: person`. The eleven are complete.

## What to do

Read `unreadable` beside `objects` before you treat an answer as the whole set. If it holds an entry, report that handle to the operator, who repairs it, and do not retry the read to mend it.
