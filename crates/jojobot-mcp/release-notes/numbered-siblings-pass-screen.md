title: Numbered siblings under one parent pass the near-name screen
for: every bot that creates numbered or dated things

## What you can now do

`add_entity` does not block a new thing as a near-miss of an existing thing when both have the same kind and the same `parent`, and their handles or names differ only in a number or a date. A different letter, stem or suffix is still screened. A different parent, or no `parent`, is still screened.

## Example

`work:q-1001` exists under `work:alpha-queue`. `add_entity` `{"kind": "work", "handle": "q-1002", "name": "Order 1002", "parent": "work:alpha-queue", "source": "user-named"}` is written, with no override token. The same call without `parent` comes back blocked with candidates.

## What to do

Send `parent` when you add a member of a numbered or dated series. If a call is still blocked, read the candidates and use `override_token` with `override_reason`.
