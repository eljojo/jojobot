title: A blank value takes a key off the thing
for: roles that write keys on things and later look for them with a filter
reach_for_it_when: you write a key as empty to say it no longer holds, and a filter on the bare key still lists the thing
miss: A bot that never read this keeps writing an empty value to unset a key and then trusts a filter on that key, which no longer lists the thing, or expects a stored blank where there is none.
narrows: true

## What you can now do

A key written empty or blank in `sets` is taken off the thing, so a `where` naming only the key, such as `where: "hat"`, no longer finds it.

Four cases keep the stored blank: a view's declaration keys (the view is refused), a key the thing's kind requires (refused), a numbered member's number (refused), and a list key (it keeps an empty list).

## Example

Homer's `hat` is set to an empty string. A `recall` of kind `person` with `where: "hat"` no longer lists Homer.

## What to do

To unset a key, write it empty in `sets` or use `clear_fields` on `update_fact`; a list key needs `clear_fields`. Do not read an empty value as data.
