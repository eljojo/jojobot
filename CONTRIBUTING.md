# Contributing

How work lands in jojobot.

These are renderings of decisions that live in the decision log: the record of
every rule the operator has set, one claim each, which is kept in jojobot itself
and not in this repository. Where this file and that log disagree, the log wins.
A reader without the instance can take this file as the working form of it.

## Build and check

Every command runs through the flake, so a machine needs nothing but nix:

    nix develop -c make narrow CRATE=<crate> [FILTER=<substring>]
    nix develop -c make dev
    nix develop -c make help      # every target, one line each

**Between edits, run the short loop and nothing slower.** `make narrow` runs one
crate's tests and lint, and `make dev` builds and runs what the branch touches.
They are cheap, local and reversible.

**The bar runs in CI, never on a dev's host.** `make check`, `make report`, a whole
suite, the real-store suite, `make prepush` and `nix build` are slow, and some
start a database. Forgejo runs them: `make report` on every push to a `dev*-work`
branch, `make check` at the carry, and `make prepush` in the forge's prepush
workflow on the exact head. A slice is not run through them by hand.

**Green means the bar's own verdict line reads GREEN.** The workspace is
formatted, it builds, every test target passes and clippy is clean. The bar
reports every target instead of stopping at the first failure, so the count of
suites in its verdict is how much was checked. A report cites its branch run by
id and sha.

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

Commit each unit when it goes green (see Build and check). A finished diff that
needs splitting means a commit was deferred too long.

A test file added under a `tests` directory is at most 100 KB (`test_file_bytes` in
`size-ratchet.toml`, rule 661). That limit only falls.

## Every commit names its authority, in one line

Which decision authorised this: a numbered rule, the operator's ask, a review
finding, or somebody's own call, marked as overturnable.

Two reasons, and the second is the real one. An audit becomes a log search instead
of a reading of every diff. And writing the line forces the author to notice when
there isn't one — **a commit that cannot name its authority is the finding**, before
a line of it is written.

The body says only what stays true. Run results, what the author believed before,
and who found what are reports, and reports go to the coordinator's mailbox: a
message box on the jojobot instance that the coordinator session reads, written
with the `post_message` verb.

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
reachable and never that it was reached. A playbook is what a real model is
driven through against a throwaway instance, and what it asserts is what the
model left in the store rather than anything it said. The playbooks are rooms
and they live in `crates/jojobot-exercise/rooms/`. A room gives one goal and a one-line entry, and leaves the agent to find the
route. A room can also ask whether the model READ rather than guessed: plant
an answer only the intended query yields, ask the sitting to record it, and
assert on the record — arithmetic over the values that query returns, because
a wrong route then gives a wrong number rather than a near-miss somebody has
to adjudicate. Pin every value the answer is computed from, or a moved input
reddens the sitting that read correctly. They run under `make paid
PLAYBOOK=<path>`, which costs money and which the bar never invokes.

## Fixtures and examples are fictional, always

Every name, place and organisation in this repository comes from a fixed fictional
roster: characters from the Simpsons, South Park, Family Guy or Bob's Burgers,
plus greek letters. `crates/jojobot-domain/tests/fixture_roster.rs` lists the
handles in two lists, and the bar checks every handle-shaped piece of text
against them. `ROSTER` is the cast. `ORDINARY` is the handles that name nobody: a
view, a work item, a label for a test. A person, pet, place or organisation on
`ORDINARY` has to say it names nobody by starting its slug with `contract-`, or
be listed in `EXEMPT` with the reason a rename cannot work. A character a test
needs is a roster name. A name written in ordinary words is not caught by that
check, so it is held by attention. This holds for tests, fixtures, comments, documentation and commit
messages alike — a commit message is part of the repository the moment it is
written.
