title: A parent that names nothing is refused, not answered empty
for: every bot that reads by parent

## What you can now do

A `recall` whose `parent` names no thing is refused with the nearest handles, as a subject that names nothing is. A parent that exists and holds nothing still answers empty.

## Example

`recall` `{"kind": "work", "parent": "project:springfield"}` is refused while no `project:springfield` exists, and lists the nearest handles it does hold. The same call answers with its work once the project exists, and with an empty answer if the project holds none.

## What to do

If your process reads an empty answer as "no work", check that the parent exists. A refusal names the handle to use.
