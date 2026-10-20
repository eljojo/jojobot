title: A type that changes how a held key folds is refused
for: every bot that declares types

## What you can now do

`declare_type` refuses a declaration that would change how a key folds when things already hold that key. A key folds one way for the whole instance, so making it a `sum` or a `newest` key changes what those things read. The refusal is `status: blocked`, writes nothing and names each key with the number of things it would change.

## Example

`declare_type` `{"name": "kettle-log", "fields": [{"key": "visits", "holds": "number", "folds": "sum"}]}` is refused while `thing:kettle` holds `visits`. Send the same call with the `override_token` from the refusal and an `override_reason`, and it lands. The receipt lists `fold_moved` with `key`, `things` and `now`.

## What to do

Declare the fold of a key before things hold it. If you want the change, send the token and a reason.
