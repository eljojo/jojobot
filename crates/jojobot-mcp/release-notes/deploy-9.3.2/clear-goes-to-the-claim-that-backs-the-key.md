title: A clear goes to the claim that backs the key
for: roles that take a key off a thing with clear_fields on update_fact
reach_for_it_when: update_fact refuses a clear and names another claim that holds the key
miss: A bot that sends clear_fields to any claim carrying the key now meets a refusal where earlier builds took the key off the thing and erased a newer value another claim held. It cannot tell, from the refusal alone, that the key is held by a different claim.
narrows: true

## What you can now do

A `clear_fields` sent to a claim that carries the key but does not back it is refused, and the refusal names the claim that does back it by its full address, `kind:slug#fN`. Before, the clear landed on the older claim, took the key off the thing, and erased the newer value another claim held.

## Example

Homer's `mood` was written as `content` on one claim and later as `queasy` on a newer one. A `clear_fields` of `mood` sent to the older claim is refused, naming the newer claim's address. The same clear sent to that address lands.

## What to do

Before you clear a key, `recall` the thing with `backing: true` and the key in `keys`: the key names the claim that backs it. Send `clear_fields` to that claim's full address. Sending the same clear again to the claim the refusal named as older meets the same refusal. A counter key has no single backing claim: `history` on the key lists the claims that wrote it.
