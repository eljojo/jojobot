title: A thing you were already served comes back as its handle alone
for: roles that read the same thing again in one session to see what it holds
reach_for_it_when: a read returns only a handle or an address where you expected the fields
miss: A bot that reads a thing again to see what it holds gets only a handle back and may take that for an empty or missing answer.
narrows: true

## What you can now do

In one session, `recall` and `search` serve an entity or a claim you were served earlier, still as it stood, as its handle or address alone. `list_entities` does the same for an entity. Send `reship: true` and the same call returns it whole.

## Example

Marge reads `person:homer` with `recall` and gets the fields. Later in the same session Marge asks again and `person:homer` has not changed: the answer is the handle `person:homer` and nothing else. Marge sends the call again with `reship: true` and gets the whole entity.

## What to do

When a read returns a bare handle or address for something you already read in this session, it has not changed since you were served it, so use what you hold. Send `reship: true` when you need it served in full again.
