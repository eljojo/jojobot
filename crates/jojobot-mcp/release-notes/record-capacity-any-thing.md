title: Any thing can carry a ceiling on its live records
for: every bot that keeps records on a thing

## What you can now do

`record_capacity` is a whole number of one or more that caps how many live records a thing of any kind holds. A thread holds three unless it sets its own number. A capture that would pass the ceiling is refused with nothing written. The refusal names `ceiling_from`, `setting` or `default`, and the routes: rewrite, archive, split and document.

## Example

`add_entity` `{"kind": "topic", "handle": "rules-springfield", "name": "Springfield rules", "source": "user-named", "sets": {"record_capacity": "2"}}` makes a topic that holds two live records. A third `capture` on `topic:rules-springfield` is refused with `"ceiling_from": "setting"`.

## What to do

If a topic or a project grows without end, give it a `record_capacity`. When a capture is refused for a ceiling, take the route the refusal names.
