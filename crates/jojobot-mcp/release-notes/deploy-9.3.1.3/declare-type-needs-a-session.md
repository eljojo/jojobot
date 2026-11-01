title: declare_type needs a session
for: roles that declare types for an instance
reach_for_it_when: declare_type comes back refused when it landed before
miss: A declare_type call with no session is refused, and it landed before. Start a session with start_here and send its sid.
narrows: true

## What you can now do

`declare_type` is now an identified write, as every other write that names its author is. Start a session with `start_here` and send its `sid` with the call: the declaration lands as it did.

## Example

Bart declares a type with no `sid`, and the call is refused with a way forward that names `start_here`. Bart starts a session, sends its `sid`, and the declaration lands.

## What to do

Send the `sid` of your session with every `declare_type`.
