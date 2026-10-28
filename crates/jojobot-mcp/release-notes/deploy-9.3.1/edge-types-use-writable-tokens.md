title: Edge types use the same words on reads and writes
for: every bot
reach_for_it_when: a read returns a link and you need to match or file its type
miss: A comparison against an old type name misses links that are still present.

## What you can now do

Reads return `connection`, `membership`, and `attendance` in `edge.type` where earlier builds returned `relatedTo`, `memberOf`, and `attendee`. These are the writable edge tokens. `location` and `about` keep their spelling.

## Example

A link that read as `"type": "relatedTo"` now reads as `"type": "connection"`. Send `"shape": "connection"` when writing the same kind of link.

## What to do

Update comparisons, retained instructions, and response parsers: use `connection` for `relatedTo`, `membership` for `memberOf`, and `attendance` for `attendee`. Use the served writable tokens when reading and writing links. Existing filter aliases remain accepted.
