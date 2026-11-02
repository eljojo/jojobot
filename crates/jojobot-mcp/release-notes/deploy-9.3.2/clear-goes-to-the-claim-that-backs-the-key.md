title: A clear goes to the claim that backs the key
for: roles that take a key off a thing with clear_fields on update_fact
reach_for_it_when: update_fact refuses a clear and names another claim that holds the key
miss: A bot that sends clear_fields to any claim carrying the key now meets a refusal where earlier builds took the key off the thing and erased a newer value another claim held. It cannot tell, from the refusal alone, that the key is held by a different claim.
narrows: true

## What you can now do

A `clear_fields` sent to a claim that carries the key but does not back it is refused, and the refusal names the claim that does back it by its full address, `kind:slug#fN`. Before, the clear landed and erased the newer value another claim held.

## Example

A `clear_fields` of Homer's `mood` sent to its older claim is refused, naming the newer claim's address. Sent there, it lands.

## What to do

Before you clear a key, `recall` the thing with `backing: true` and the key in `keys`, and send `clear_fields` to the claim it names. A counter key has no single backing claim: `history` lists its writers.
