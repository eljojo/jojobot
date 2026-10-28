title: Memory writes return receipts instead of whole records
for: roles that write claims and then need to inspect what was stored
reach_for_it_when: a successful write answers briefly and you need the claim itself
miss: A bot that never read this could expect echoed records or enable smaller search answers without realizing that the same setting also shrinks write receipts and defers notices.
narrows: true

## What you can now do

Ordinary single `add_entity`, `capture`, `update_entity` and `update_fact` calls return receipts rather than echoing whole records. A normal successful receipt identifies the object, keeps a claim address when the write supplies one, and reports changes and new or derived information. Read the object or claim when you need its stored words and fields. This applies to single writes, not `entries` or `ticks` batches.

The calling bot's `setting_token_saving` key controls both search answers and these write receipts. With `on`, a successful write returns only `id` and optional `derived` values; a revision, when present, is under `derived`. Claim addresses and change notices are omitted from that receipt. Notices wait for an ordinary answer; refusals still explain how to proceed.

## Example

If a normal `capture` receipt returns `person:milhouse#f1`, read that claim with `recall` `{"records":["person:milhouse#f1"],"details":true}`. If a compact receipt returns only `{"id":"person:milhouse"}`, use `recall` `{"subject":"person:milhouse","facts":true,"details":true}` to read its claims and their addresses.

## What to do

Keep `setting_token_saving` off when you need ordinary receipts and their notices; `off` is the default. Set it to `on` on your bot only when both smaller search answers and smaller write receipts suit your work. If the setting cannot be read, a write uses normal mode and says why. A brief successful receipt acknowledges the write; it is not a copy of the stored claim.
