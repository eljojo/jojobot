# CLAUDE.md — working in this repo

## What jojobot is

jojobot is a personal-assistant **domain server**. It fronts a user's existing
life-layer services (kanban, notes wiki, calendar, link library) and exposes
itself to an AI assistant through one MCP endpoint. It is not an MCP proxy: it
is the assistant's *method*, made into software — it feeds the assistant
context, guards writes behind invariants, remembers everything, and never
pretends to think. The assistant is the only mind.

**The through-line:** the assistant's behaviour today lives as prose — markdown
rules, skills, per-session caches, boot hooks. The mission is to migrate it
into software: versioned, tested, enforced. A fuzzy rule in a markdown file
becomes a typed invariant the compiler and tests hold. The end state is
sessions that boot *from jojobot* instead of from files.

**One server instance, the only writer — so every read comes from memory.**
jojobot runs as exactly one process, and every write passes through it. That
is why the whole fold is held in memory and every read is answered from it
alone; each write updates the store and the fold in the same step, so the
fold is always current. **No read goes to the store.** ⚠️ A second writer of
any kind — another worker, a restore, a migration tool, a person running SQL
— makes the memory copy silently wrong. (Decision log 292.)

## ⚠️ THE BRIGHT LINE — zero user PII, zero life specifics. Ever.

**This repo is public. Nothing that identifies a user's life enters it —
no real people, places, events, organizations, festivals, trips —
anywhere:** code, tests, fixtures, docs, commit messages, branch names,
error strings. The line binds text *bound for* the repo, not just files
inside it: a hand-off task, an example, a report that will become a commit
message is covered the moment it is written. Fixture names come from the
fictional roster — characters from the Simpsons, South Park, Family Guy or
Bob's Burgers, plus greek letters —
`crates/jojobot-domain/tests/fixture_roster.rs` is the allowlist and the
test bar enforces it. The near-miss screen matches on containment, a known
nickname, and two or more shared rare words, so a new handle is refused when
an existing one is a prefix of it — qualifying a name already on the roster is
the way to trip it, and so is a second fixture sharing two words with the first
("Grant boot-as-patty" beside "Grant boot-as-nelson"); lift it with the token
and a reason, as the engine's own records do. Read the guard's own
answer instead of guessing at a rename. An example quoted from the user's
private docs gets a roster substitution BEFORE it crosses; that quote path
is the standing leak vector and has burned this project three times. When
in doubt, it doesn't cross.

## Where the design lives

- **The roadmap is the work-queue board**, not a document. Every release, rock
  and slice lives there; a release that exists only in prose is one nobody
  works. **The decision log** — every rule the operator has set, one claim each —
  **lives in jojobot itself**, under its decision-log topic, each rule on the
  rules topic of its subject; read it with `recall`. **The product brief** that
  renders it lives in the operator's private wiki, not in this repo. **The per-feature briefs DO live here, at
  `pm/feature-briefs/`** — one file per feature, saying what it is for, why it is
  shaped that way, and what it deliberately is not. **Read the brief for a
  feature before you reason about that feature**: it usually already answers what
  looks like a fresh design finding. The coordinator session owns all of them and
  reconciles them after every slice. There is no architecture document and no
  capability list: as-built detail is the code and the served texts in
  `crates/jojobot-mcp/served-texts`, which are exactly what a model reads. What
  ships in which release lives on the release items in jojobot.
- **The README's "Where it is" rows** say which features work, partly work or
  are not built, measured against `main`; a slice that moves a feature between
  those states moves its row in the same round.
- **Migration note:** while behaviour migrates, most operating context still
  lives in a private repo of the user's. Sessions on the user's machine may read
  it for orientation. **Nothing life-specific may cross back into this repo** —
  see the bright line below.

## The build model — who does what

- **PM (`pm`)** — holds the vision, scopes each card, writes the hand-off,
  reviews adversarially, reconciles the docs. Does not implement.
- **EM (`em`)** — decides which line takes which card and when, dispatches,
  reads the reports, accepts and carries (decision log 358, 451).
- **Implementer (a dev line: `dev`, `dev7`, `dev-sol`, …)** — boots in this
  repo as the bot its start line names, builds ONE scoped card test-first,
  reports, and is disposed. If that's you: your task is in **your own bot's
  mailbox** (`read_mailbox`). When done, `mark_processed(message, notes)` and
  post your report where the dispatch says: delivery to the **`em`** box, a
  scope question to the **`pm`** box. A failure is also a report — failure is
  data, not silence.
- **Audience discipline — the user is not the report channel.** Detail —
  commits, deviations, test output, design notes — goes to the box your
  dispatch names, in full. The user gets the close-out only: done or blocked,
  in a line or two. Don't narrate the work to the user as you go; if something
  needs a decision mid-card, ask it as one crisp question.
- Merging advances `main` only. **Deploy and push are the user's verbs** —
  never deploy; never push unless asked. The one exception: a dev line pushes
  its own branches to Forgejo (see "A dev line manages its own git" below).

### Booting a session in this repo

A session here starts at jojobot, not at these files. The whole protocol:

1. **`start_here` with your bot name** — the one your start line names.
   It hands back a `sid`; pass it on every call after that, reads included.
   It also hands back your charter and your rules, which win over this file
   wherever the two differ.
2. **`read_mailbox`** — your task is in your own box. Work it.
3. **Report where the dispatch says** as you go, not only at the end, and
   `mark_processed` a task the moment you have acted on it.
4. **Stop when your charter says.** Some lines take one card per session and
   wrap; others poll for the next with `read_mailbox` `counts_only: true`,
   which costs nothing and takes delivery of nothing. Your charter decides,
   not this file.

## Engineering rules (non-negotiable)

- **Strict TDD.** A feature is proven by an automated test; every bug fix
  starts from a failing test you watched fail FIRST. A manual run proves
  nothing.
- **To watch a test fail, use `scripts/sabotage`.** It keeps the file, makes
  the edit, runs the command, prints the verdict and puts the file back.
  **The restore is on a trap, so it happens on a pass, a failure, a raise or
  a stop** — there is no dirty tree to clean up afterwards and no moment
  where a git verb is the convenient answer. **It refuses unless the text
  appears exactly once**, and it reports an absence and an ambiguity apart,
  because those send you to different places. **It also refuses when its own
  classifier fails its self-check** — a third reason, so read which one you
  got before hunting for an ambiguity that is not there, and a refusal is
  said on stdout as well as stderr, so filtering one stream never makes a
  refusal look like silence. **It refuses a run in which no test executed**,
  before any verdict, because a filter that selects nothing stays green under
  every break. **Each run keeps its own in-flight record**, naming its
  checkout and process, so two lines that sabotage at once never lose each
  other's. **A green verdict is cross-checked against the
  run's own output**, and a disagreement between the two is printed rather
  than enforced — the exit code is still the verdict. ⚠️ **A hard kill
  cannot be caught, so it names the copy on stdout when it starts: that name
  is what you copy back over the file, and no git verb is involved.**
- **A sabotage proves nothing until you know it landed where you meant.** Two
  sites that look alike, one edit, and the verdict is about code nobody
  touched. **Assert the edit reached THAT site**, and prove the case moves when
  its own guard breaks and stays still when an independent one does. ⚠️
  **Choosing an independent neighbour is the hard part — reasoning about which
  one is independent is not enough. Run it.** ⭐ **And the same holds for which
  of two mechanisms is producing a result you are about to build on:** a
  plausible, documented, entirely real hazard can be the wrong explanation for
  the case in front of you, and only printing what the code actually saw tells
  the two apart.
- ⚠️ **And a check that reads a merged file is only as good as the REF you read
  it out of.** `main` is not a name, it is two refs: the local branch and the
  remote-tracking one, which can be a hundred commits behind. **Reading the
  wrong one gives a false green and a false alarm in the same minute** — a
  commit read out of your own branch proves your commit contains your commit.
- 🚨 **A needle that is a PHRASE is weakened by approximate matching.** Search
  matches by stem and needs most of a query's terms, so a multi-word needle can
  match a record that shares one word with it. **Assert on one distinctive
  word**, and never rest an ABSENCE on a phrase — that case can go green over
  nothing the day the matcher loosens again.
- 🚨 **Asserting a field is present and non-empty says nothing about whether it
  can be READ.** A wrapped string literal whose continuation collapses passes
  every such check while serving runs of spaces mid-sentence. **For a field
  whose whole purpose is to be read, assert its shape** — one line, no double
  space — because that is most of what it is for.
- 🚨 **A GREEN sabotage is not a control until you read the line under it.** A
  case can execute the mutated line and not depend on what it computes, which
  looks identical to the code being irrelevant. **So on a green verdict the
  tool probes again with a panic at the same site and says which kind of green
  it was** — depended on, blind, or a site no panic can be written into.
  **Read that line: a green with no probe line under it means the tool could
  not answer, not that the case is sound.** It roughly doubles the run and it
  never gates the verdict.
- 🚨 **A FIXTURE THAT WAS INVENTED RATHER THAN OBSERVED WILL PASS WHATEVER THE
  CODE DOES WITH IT.** A case is only as good as the shape it is fed: hand it a
  structure the real system never produces and it exercises a world that does
  not exist. ⛔️ **The failure is silent and it protects the very bug it was
  written against** — a check that scanned text for a date passed fixtures with
  no address at all, because scanning text is all it did, and four of its own
  cases went red the moment it was made to read structure. **Take the shape from
  something the system actually emitted.** ⭐ **This is the hostile-double rule
  one layer down: that one says a stand-in must lie where reality lies, this one
  says the INPUT must look like what reality hands you.**
  **Read the mutation before you read the verdict, too** — ask what value it
  changes. A mutation that computes the same thing is a no-op, and the probe
  above cannot see it: the site executes, a panic there would be noticed, and
  the tool truthfully reports *depended on* about a change that changed nothing.
- 🚨 **A constant asserted through itself pins NOTHING.** An assertion whose
  expected value is the same constant the code under test used proves the code
  consistent with itself and says nothing about the VALUE — rename the constant
  and both sides move together. **That matters where the spelling is STORED or
  SERVED and nothing outside this process declares it**: a key in a fields bag,
  a marker inside a stored body, a token on the wire, a phrase a record is
  parsed back through. The store accepts the new spelling, only records written
  before the change disagree, **and a record cannot fail a test. There, one test
  pins the literal.** Where the far side DOES declare the spelling — a column, a
  table, a path, a zone, a binary on the PATH — **the real-dependency suite
  already pins it and a literal adds nothing**, because a wrong value fails the
  first time that dependency runs. **The question that tells the two apart is
  one question: does anything outside this process declare this spelling?**
  ⭐ **A pin can also be a property that determines the value uniquely rather
  than a literal.** ⛔️ **It is a rule and not a lint:** the predicate that would
  automate it — an assertion whose expected value is an identifier from the same
  crate — flagged 62 sites for 0 real, because whether a value is pinned lives
  in a different file and often in a case written about something else.
  **5 unpinned spellings across 3,697 assertions.**
- 🚨 **Check the exit path BEFORE a run whose expected verdict is GREEN,
  because nothing afterwards will.** Distrusting a surprising result catches a
  broken instrument; **belief in an expected one is not a signal at all**, and a
  sabotage you expect to come back green has no red in it to catch a verdict
  that was never measuring. **A pipe swallowing the runner's exit code prints
  exactly the answer you were hoping for.**
- 🚨 **A room runs against a BUILT binary, so a sabotage of served code is
  invisible to it unless you build first.** The rooms spawn the server, and
  `cargo test -p jojobot-exercise` does not rebuild it — **so an edit to any
  served source, checked only through the rooms, reads GREEN because the code
  under test was never loaded.** That is the strongest possible wrong reason
  for a pass. **Build the workspace in the same command as the run** —
  `make rooms` and `make room` do. ⚠️ **And
  the same trap reads the other way: a REFUSAL you did not expect may be the
  old binary publishing the old schema rather than a second copy of a rule.**
- ⚠️ **An exit code belongs to the LAST command in the pipeline, so any verdict
  you read through a filter is reporting the filter.** A run piped through
  `grep` reports `grep`'s exit code, and a tool handed that pipeline faithfully
  repeats it. **It lies in BOTH directions and the second one is the surprise:**
  a `grep` for failures that finds none exits 1, so **the cleaner the run, the
  more certain the false alarm** — and a `grep` that finds a failure exits 0,
  which reads as green with the failing names printed directly above it. ⛔️
  **The instrument is not at fault and must not second-guess what it was
  given.** ⭐ **Do not pipe the verdict at all: redirect to a log and read the
  code — the only form with nothing to get wrong.**
- **Inserting code directly above an attribute orphans it onto what you
  inserted.** A `#[test]` or `#[cfg(test)]` line binds to the item below it, so
  a new item slipped in between takes the attribute and the old one loses it.
  ⚠️ **One instance ran a case TWICE while every suite stayed green** — a
  duplicated attribute is not a failure, it is a count nobody reads. **The
  linter finds these and the suite cannot.** Insert below the attribute, or
  read the two lines above your edit before you leave it.
- **Every feature appears in a user story.** A feature that no story exercises
  is a finding. This is a second bar, not the same one: a unit test proves the
  feature works, and a story proves the feature can be reached through the
  served surface. A capability can hold full unit coverage and still be
  invisible to every session that did not already know it was there. **A call
  with no assertion is not coverage** — a beat asserts what came back, what
  changed, or what a later read returns. A case that only checks where the
  surface stops or what shape a line takes — a size limit, the answer ceiling,
  a boot budget — carries no beat a person follows: it lives in a `contract_*`
  file beside the stories, headed as a contract, and does not count as a story.
  **A story's server starts the way the binary does** — the memory image is
  filled on every start through the same wiring call boot makes, and the
  startup passes (the handle registry, the keys of runs that have not written,
  the prune of their spent notes, the default identity, the instance record and
  the shipped types) are one function, `run_startup_passes`, that the binary and
  the story server both call — so a story that writes to the store beneath the
  served image calls `the_store_changed_underneath`, as a restart would, or it
  tests a server that cannot exist.
- **And a third bar, which neither of the first two can reach: whether a session
  that was told nothing FINDS the capability.** A story is written by somebody
  who already knows the answer, so it can only prove a capability is reachable —
  never that it was reached. A **playbook** is what a real model is driven
  through against a throwaway instance, and its assertions are over what the
  model LEFT in the store, never over what it said. `make paid` takes the
  playbook as a parameter, and the playbooks are **rooms**, in
  `crates/jojobot-exercise/rooms/`. **A run is KEPT** — written whole to a file
  the run names, before its exit code is decided, because a run that failed its
  expectations is the one most worth reading. `TRANSCRIPT=` puts it elsewhere.
  **A run's report and transcript end with where its time went** (a replay's
  too): wall time, model turns, tool calls, and the server's executing time with
  its share of the wall, read from the run's own clock and request log.
  **A run that produced nothing says so in its own words**, so an empty file can
  only mean the capture never wrote — a different fault with a different fix.
  ⚠️ **This is what makes a run something to judge by after the fact rather
  than something somebody had to be watching**, and the judgement stays a
  person's: no score, no rubric, no summary. A room gives one goal in the operator's
  voice and a one-line entry, then leaves the agent to find the door, the box
  and the route. **A room is its own document: its starting world, its
  checks and the server day it runs on are written there.** A check that cannot be asked of jojobot is an
  escape into Rust, and a run prints how many escapes a room took, including
  none. It is in addition to the stories, never instead of them.
- **Commits: one per coherent problem.** A slice lands as a handful of
  commits — never one per file or checklist item, never dozens. **A fix and
  the test that proves it are ONE problem**, however a task listed them.
- **A safe rebase needs no one's permission.** Rebasing unpushed commits onto
  the main line, and joining them by fast-forward, is routine work. Never make
  a merge commit. Push and deploy stay the operator's.
- **A dev line manages its own git, and pushes its own branches to Forgejo.**
  This is the operator's standing grant, so a line asks nobody first. In its
  own checkout and on its own branches (`devN-work` and any `devN-*` branch it
  made), a line uses any git verb: rebase onto main, create, switch, reset and
  delete its own branches. It pushes those branches with
  `git push forgejo <branch>`, and a `--force-with-lease` after its own rebase
  is included. Not covered: `main`, another line's branch or checkout, any
  remote other than `forgejo`, and a merge commit. `main` moves only by the
  engineering manager's carry.
- **A commit message uses Simplified Technical English**, like every other text
  this project writes. Short sentences. Active voice. One word has one meaning.
  No metaphor.
- **The body states what the code does now and why the change is correct.** It
  does not state who asked for it, which message carried the request, what
  anyone believed before, what a run printed, or how many attempts it took.
  **Strike every sentence that would be false next month.** Those sentences are
  a *report*, and a report goes to the coordinator.
- **Cite a rule number when a rule governs the change.** Write no attribution
  when no rule applies: every commit here was asked for, so "the operator asked
  for this" carries nothing. Never cite a message id — mailbox traffic is
  ephemeral and a later reader cannot resolve it. Never write a pronoun for the
  operator; this repo names roles.
- **A diagnostic is not a test and is never committed.** Something run once to
  answer a question — a probe, a scan, a count — is a script. It guards
  nothing and will never fail meaningfully again. Run it, put the *answer*
  where the question lives, delete the instrument. Enforcing the answer later
  is a deliberate test with an assertion, and a different artifact.
- **Zero user PII, zero life specifics** — the bright line at the top of this
  file. It outranks every other rule here.
- **The real-dependency gate.** A slice that touches an adapter does not merge
  on fakes alone: the suites that exercise a real store must have run on it,
  and the branch's CI run is where they run (a dev line never runs a whole
  suite locally; see the bar below), so the report cites that run. **They need no credentials and no separate gate** — every store
  jojobot fronts is a process it spawns itself, against a disposable database,
  so the suites are ordinary cases the whole run already covers. **That makes
  the gate cheap, which removes the last excuse for skipping it**: a suite that
  exists and was never run is a blocker, not a footnote.
- **Fakes must be hostile where reality is.** When a real store's quirk is
  discovered (normalization, silent drops, clamped pagination), bake it into
  the fake or a golden fixture — a polite fake that stores bytes verbatim is
  how green tests ship broken adapters.
- **Hexagonal, domain-driven.** `jojobot-domain` stays pure (no I/O, no MCP);
  each fronted service's quirks live in its adapter, quarantined.
- **Green bar before DONE, and the bar runs in CI, never on a dev host.** The
  bar is `make check`: `cargo test` green and `cargo clippy` clean, through the
  flake. **Within one job it reports every target rather than stopping at the
  first failure, so its count is coverage** — a red run names each suite's verdict instead of
  leaving a reader to guess which parts ran. **A dev line never runs `make
  check`, `make report`, a whole suite, the real-store suites, the upgrade gate,
  `make prepush` or `nix build` locally:** between edits it runs its own cases
  (`make narrow CRATE=<crate> [FILTER=<substring>]`), a suite only when it
  expects that suite to fail or is debugging a red, then pushes its branch and
  cites that branch run by id and sha. The full bar runs once per change, at
  the carry (rules 501, 622). **In CI a job that goes red cancels its sibling
  jobs**, so a cancelled job in a red run is unproven, never passed.
- **The size ratchet** (`size-ratchet.toml`, checked by `bar size`): a file over
  `cap` lines (5,000) is red and a new one is refused; over 2,000 is yellow and
  warns. **Any file may grow when the change honestly needs it, with the reason
  in the report (rule 741)**; the limits never force a worse design.
  `test_file_bytes` caps an added test file at 100 KB (rule 661). Both only fall.
- **The bar has levels, and `make check` runs no room.** Each suite that
  stands up a room sits behind a cargo feature, so the bar compiles and lints
  it and never runs it. `make rooms` runs every room but the year,
  `make room ROOM=<name>` runs one, and `make year` runs the year room. **Forgejo CI
  runs them**: a push to a `devN-work` branch runs `make report` (the check,
  less the slow jobs its recorded history lets go) and `make rooms` beside it,
  so a defect where two cards meet shows at the dev's report, and a pull request to
  `main` runs `check`, then `rooms` and `year` — the three checks `main`
  requires, so renaming a job drops a requirement. Every run keeps
  `target/bar/*.log`. `make prepush` is release QA and runs only in the forge's `prepush`
  workflow, dispatched on the exact head, never on a dev host: the
  bar, the real store, the rooms, the year room, `nix build` and `nix flake check`, reporting every step. **A slice that
  touches a room's document, a room's test file or the exercise harness cites
  the `make rooms` job of its branch run in its report** — the bar alone
  cannot see it, and the rooms run in CI, not locally. **`make check` and `make rooms` run their test binaries
  side by side**, eight at a time unless `BAR_JOBS` sets another number, and
  the verdict names the number; `make test` is the plain serial run. Each
  run's test processes get a run root as their temp directory, under
  `/dev/shm` when it has 2 GiB free and on disk otherwise, with one line
  saying why it fell back.
- **An upgrade is proven by a test, never by a rehearsal on a copy of
  production.** The upgrade gate restores the store the last deployed release
  filled, boots the current binary on it, reads it back through the served
  surface, and fails on any warning or error line at boot. **That store is
  built in CI from the release's `deploy-N` tag and cached outside git; no
  fixture or dump is ever committed (rule 661).** Only the last release
  matters, because production upgrades only from the build it runs. A cache
  miss rebuilds it; it never skips the gate. **The Nix sandbox check cannot
  record or restore that store**, so `flake.nix` skips the gate's cases, the
  interrupted-start `deaths::` cases and the others that need git history or a
  running server; `make check` in CI runs every one of them, and it is the gate.
  **It also boots the head over a start interrupted partway through each
  migration the head adds**, in both states a death leaves (the marker alone,
  and the change landed with no ledger row), built by `leave_interrupted`, the
  one helper the migrate unit cases share. `make refresh-upgrade-fixture
  REF=<deploy tag> DIR=<cache directory>` is CI's recording step, never the
  local loop's. An empty
  `RUST_LOG` counts as unset. **`make carried` says what a carry would move**:
  this checkout's commits that main lacks, main's commits past origin/main, and
  whether main moved on.
- **`make paid` is a third tier and `make check` never runs it.** It reaches the
  network, drives a real model through a playbook against an instance it spawns
  and throws away, and it costs money. **It must never run by accident**, which
  is why it stands apart rather than hiding behind an ignore marker. It needs
  the agent CLI on the PATH; this repo does not provision one.
- **A change to served text commits the files `make snapshot` rewrites**
  (`crates/jojobot-mcp/served-texts`): `make check` fails on any difference
  between them and what a model reads. It also lints the migrations (one change
  per file) and the store passes (a changed pass takes a new name).
- **A test must travel the path it claims to exercise.** Calling a function
  directly proves the function, not that it is wired to anything. Assert that
  a value survives a journey by sending it on the journey, through the surface
  a caller uses — otherwise the assertion holds identically on a build where
  the value never moves.
- **Never pin our own prose.** An assertion quoting a sentence somebody wrote
  breaks when the sentence improves and proves nothing about behaviour. Pin
  identifiers and structure — things a rename would be a real change to. The
  tell is a needle that is a phrase rather than a name.
- **Don't over-engineer.** Reach for the simplest model that fits the stated
  design; the tell is a pass that keeps getting bigger. When the design and
  the code disagree, say so — don't silently deviate, and don't silently
  comply either.
