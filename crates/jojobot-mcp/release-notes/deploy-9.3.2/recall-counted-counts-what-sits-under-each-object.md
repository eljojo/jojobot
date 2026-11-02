title: recall's counted argument counts what sits under each object
for: roles that read a tree of things and want its size and its last write without reading every child
reach_for_it_when: you list a project, a kind or a set of roots and need how much sits under each and when it last changed
miss: A bot that wants a count or a last-write moment lists the children and counts them itself, or reads every claim of each thing, and pays for a long read that the server can answer in one pass.

## What you can now do

`recall` takes a boolean `counted`, off by default. With it true, every object that has something under it carries `children` (`count` of live children, `archived`, and `subtree`, every live thing under it), and a thing with nothing under it carries none. An object also carries `written_at`, the moment of its newest write, where the store has one. A read of one subject carries `children` without it, for a parent.

## Example

`recall` with `kind` "project" and `counted` true returns each project with `children` (`count` 3, `archived` 1, `subtree` 8) and `written_at`.

## What to do

Send `counted` true when you need sizes or last-write moments, and read `children.subtree` and `written_at`. Leave it off otherwise.
