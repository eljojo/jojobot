# Contributing

How work lands in jojobot.

These are renderings of decisions that live in the decision log. Where this file
and that log disagree, the log wins.

## Two sessions, never one

A **coordinator** holds the vision and the context, scopes each slice, writes the
hand-off, reviews adversarially, and reconciles the docs afterwards. It does not
implement.

An **implementer** takes one scoped slice, builds it test-first, reports back, and
is thrown away. It never holds the whole picture and never needs to.

The loop: scope, build, review, merge, reconcile.

## Deploy is the operator's verb

Merging advances `main`. Going live is the operator's move and nobody else's.

## Reviews batch to the deploy boundary

Not to each round. A full adversarial review is expensive, so a multi-round slice
gets one, run when the whole thing is believed ready to ship.

Between rounds the coordinator still reads the work and hands back only what it is
confident about — a spec difference, a contradiction, a verified divergence. That
is cheap and it is not a review.

What comes *out* of a review does not batch. Findings go as they are ready.

## Keep the slice small

A milestone lands as a handful of commits, not dozens. One commit per coherent
problem — never one per file, never one per checklist item. A fix and the test
proving it are one commit.

Commit each unit when it goes green. A finished diff that needs splitting means a
commit was deferred too long.

## Every commit names its authority, in one line

Which decision authorised this: a numbered rule, the operator's ask, a review
finding, or somebody's own call, marked as overturnable.

Two reasons, and the second is the real one. An audit becomes a log search instead
of a reading of every diff. And writing the line forces the author to notice when
there isn't one — **a commit that cannot name its authority is the finding**, before
a line of it is written.

The body says only what stays true. Run results, what the author believed before,
and who found what are reports, and reports go to the coordinator's mailbox.

## Maintenance rounds are planned capacity

Like going to the gym. They are scheduled, not squeezed into whatever feature
momentum leaves behind.

## Nothing is built on a passing remark

A decision becomes real when it lands in the decision log. Until then it is a
conversation, however enthusiastic.

## Every feature appears in a user story

A feature exercised by no story is a finding. Stories are integration tests over
the served surface, written as a scenario somebody actually lives through, and
their job is to discover what a real use case cannot do.

A call with no assertion is not coverage. Where a beat needs something that does
not exist, it is marked as a gap rather than worked around, and the marker goes
red on the day the capability lands.

There is a third bar above this one, and stories cannot reach it. A story is
written by somebody who already knows the answer, so it proves a capability is
reachable and never that it was reached. `COLD-SESSION-SUITE.md` is the script a
real model is driven through against a throwaway instance, and what it asserts is
what the model left in the store rather than anything it said. It runs under
`make paid`, which costs money and which `make check` never invokes.

## Fixtures and examples are fictional, always

Every name, place and organisation in this repository comes from a fixed fictional
roster. This holds for tests, fixtures, comments, documentation and commit
messages alike — a commit message is part of the repository the moment it is
written.
