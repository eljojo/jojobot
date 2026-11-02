title: recall's counted argument counts what sits under each object
for: roles that read a tree of things and want its size and its last write without reading every child
reach_for_it_when: you list a project, a kind or a set of roots and need how much sits under each and when it last changed
miss: A bot that wants a count or a last-write moment lists the children and counts them itself, or reads every claim of each thing, and pays for a long read that the server can answer in one pass.

## What you can now do

`recall` takes a boolean `counted`, off by default. With it true, every object of the answer carries `children` and `written_at`. `children` holds `count` (its live children), `archived` (its archived children) and `subtree` (every live thing under it). `written_at` is the moment of the newest write of any kind on the object. A subject read answers in the same shape. A call that does not send `counted` gets neither.

## Example

`recall` with `kind` "project" and `counted` true returns each project with `children` of, say, `count` 3, `archived` 1 and `subtree` 8, and the moment its newest write landed under `written_at`.

## What to do

Send `counted` true when you need sizes or last-write moments for a list of things, and read `children.subtree` and `written_at` from each object. Leave it off when you only need the things.
