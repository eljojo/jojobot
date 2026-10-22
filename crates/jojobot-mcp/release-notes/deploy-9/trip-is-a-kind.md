title: A trip is now a kind of thing
for: roles that plan or remember journeys
reach_for_it_when: you record a journey, its places and its dates as notes on some other thing
miss: A bot that never read this would file a journey as a thing of another kind, and its dates would not be found by the keys that read them.

## What you can now do

`add_entity` takes `trip` as a `kind`. A trip carries `departs_from` and `arrives_at`, each a `place` handle, and `leaves_on` and `returns_on`, each a date. All four keys are needed for the trip to fit its type, but no write needs them. A trip with only some of them is accepted.

## Example

`add_entity` `{"kind": "trip", "handle": "springfield-1999", "name": "Springfield 1999", "source": "user-named", "sets": {"departs_from": "place:springfield", "arrives_at": "place:capital-city", "leaves_on": "2026-11-02", "returns_on": "2026-11-09"}}` creates `trip:springfield-1999`. Both places must exist first.

## What to do

Use `kind: trip` for a journey. Do not call `declare_type` with the name `trip`, because the name now belongs to the kind.
