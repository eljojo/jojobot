title: A clear sent to an older claim leaves the newer value on the thing
for: roles that take a key off a thing with clear_fields on update_fact
reach_for_it_when: you sent clear_fields to a claim that carries the key and were refused before, or you rely on the refusal to learn which claim backs a key
miss: A bot that relied on the refusal of a clear sent to the wrong claim will not get one. The clear now lands, and the key may still be on the thing.

## What you can now do

A `clear_fields` sent to a claim that carries the key but does not back it is no longer refused. It takes that claim's own value of the key out of play and nothing else: the thing keeps the value of the newer claim that backs the key. Sent to the claim that backs the key, a clear takes the key off the thing, and an older value does not come back. The receipt says which of the two happened for each key: that the key is off the thing, that the key stays on it held by a named claim, or that it was already off and the call changed nothing. This replaces the refusal described in the deploy 9.3.2 note "A clear goes to the claim that backs the key".

## Example

Homer's `mood` was written by an older claim and then by a newer one. A `clear_fields` of `mood` sent to the older claim lands, and the receipt says `mood` stays on Homer, held by the newer claim's address. Sent to the newer claim, the same clear lands and the receipt says `mood` is off Homer.

## What to do

Read the receipt of a clear, not only that it landed: it says whether the key is off the thing. When the key must be off, `recall` the thing with `backing: true` and the key in `keys`, and send `clear_fields` to the claim it names.
