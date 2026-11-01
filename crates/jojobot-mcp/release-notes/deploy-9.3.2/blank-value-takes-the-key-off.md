title: A blank value takes a key off the thing
for: roles that write keys on things and later look for them with a filter
reach_for_it_when: you write a key as empty to say it no longer holds, and a filter on the bare key still lists the thing
miss: A bot that never read this keeps writing an empty value to unset a key and then trusts a filter on that key, which no longer lists the thing, or expects a stored blank where there is none.
narrows: true

## What you can now do

A key written with an empty or blank value in `sets` is taken off the thing. The thing no longer holds the key, so a `where` that names only the key, such as `where: "hat"`, no longer finds it. Earlier builds kept the empty string as a value, and the thing still matched.

Four cases keep the rule from taking effect. A view reads six keys as its declaration (`selects`, `answers_type`, `follow_shape`, `follow_relation`, `follow_direction` and `follow_fits_type`): a blank there stays a stored blank, so the view is refused and does not run unscoped. A key that the thing's kind requires refuses a blank, as it refuses a clear. A numbered member refuses a blank number, because a blank number is a bad number and not an absent one. A key that a declared type holds as a list keeps its key when it is emptied: a blank leaves an empty list, so the thing still holds the key, and removing the last item of a list does the same.

## Example

Homer's `hat` was written as `bowler`. A later capture sets `hat` to an empty string in `sets`. A `recall` of kind `person` with `where: "hat"` no longer lists Homer, and a `recall` of `person:homer` no longer shows a `hat` key.

## What to do

To unset a key that holds one value, write it empty in `sets`, or use `clear_fields` on `update_fact` as before. To take a list key off the thing, use `clear_fields`: an empty list value keeps the key. Do not read an empty value back as data.
