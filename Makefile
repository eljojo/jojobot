# jojobot — the green bar, written down once.
#
# Run these THROUGH the flake, which is where the toolchain lives:
#
#     nix develop -c make check
#
# The recipes call `cargo` directly rather than shelling out to `nix develop`
# themselves, so running `make` from inside the dev shell doesn't nest a second
# one. That is also why `make` itself is in the shell's inputs.
#
# One recipe is the exception on purpose: `refresh-upgrade-fixture` builds an
# OLDER ref in a worktree of its own, and that ref has to be built with its own
# flake and toolchain, not with the shell of the checkout that ran `make`. Its
# script runs `nix develop` inside that worktree for exactly that reason.

CARGO ?= cargo

# **A pull-request job runs its bar at a fixed path.** `CI_ROOT=<path>` runs the
# bar through `scripts/ci-root`, which shows this checkout at <path>. The test
# binaries of one pull-request run are compiled by one job (`make bundle`) and
# run by the others, and a binary reads its files by the path it was compiled
# in. Unset, a target runs where it stands.
CI_ROOT ?=
AT_ROOT = $(if $(CI_ROOT),scripts/ci-root $(CI_ROOT) ,)

# **The bar a target runs.** With `BAR_BUNDLE` set the bundle holds the bar
# itself, built from this commit with the rest, and it runs from there: a
# `cargo run` in a job with no earlier build would compile the bar and every
# crate it depends on first. The path is from the checkout root, which is
# where `CI_ROOT` puts the job. Without a bundle the bar is built and run.
BAR = $(if $(BAR_BUNDLE),cargo-target/debug/jojobot-bar,$(CARGO) run -q --locked -p jojobot-bar --)

.DEFAULT_GOAL := help
.PHONY: help check dev report narrow test lint fmt fmt-check build bundle integration rooms room prepush argument-coverage paid replay refresh-upgrade-fixture carried snapshot size

help: ## List the targets
	@grep -hE '^[a-z-]+:.*##' $(MAKEFILE_LIST) \
		| sed -e 's/:.*## / — /' \
		| awk '{ printf "  \033[1m%-12s\033[0m %s\n", $$1, substr($$0, index($$0, "—")) }'

# **The boundary bar.** Run it before a report, before a carry, and before
# anything reaches a review.
#
# It is the default and it stays the default: the whole workspace, every
# suite, every target. The narrow target below covers less, and it is the
# override rather than the rule.
#
# **Runs through `jojobot-bar` rather than calling cargo here directly.**
# `cargo test --workspace` prints thousands of lines, and a caller who pipes
# that through `tail`/`head` to fit its own context keeps the doc-tests and
# the clippy banner — the part that always looks fine — while the suite
# counts, the failing names and the compile status are above the cut.
# `jojobot-bar` runs the same phases with the same flags, writes the whole
# thing to `target/bar/check.log`, and prints a short verdict built from what
# it captured directly rather than from a pipe.
#
# **The test phase runs the test binaries side by side**, eight at a time, and
# runs the same binaries the plain `cargo test --workspace` would. `BAR_JOBS=<n>`
# sets the number; the verdict's `jobs:` line says what it was. Several bars run
# on one machine at once, so raise it for a measurement, not by habit. `make
# test` is still the plain serial workspace run.
# **Every bar run keeps its full output in `target/bar/`**, and its summary prints
# the path: `check.log` for this target, `report.log` for the next, `rooms.log`
# for `make rooms`, and `narrow-<crate>.log` for `make narrow CRATE=<crate>`, for
# example `narrow-jojobot-mcp.log`. A run overwrites the last log of its own name.
check: ## The DONE bar: formatted, green, clippy-clean
	CARGO=$(CARGO) $(AT_ROOT)$(BAR) check

# **The report bar.** What a dev's report runs: the boundary bar less the jobs
# the recorded history lets go. A job leaves only with at least five recorded
# runs, no red in its newest twenty that is not called a flake, and a
# load-discounted median of thirty seconds or more; the upgrade gate never
# leaves. A red counts only when the run's recorded head is contained in `main`:
# the history holds the runs of every line of work, and a red on another line's
# unmerged head is ignored, and so is a red from a run whose tree was dirty;
# the level line counts both. With no history it runs everything the boundary
# bar runs. A carry, a review and a push run `check`, not this.
# **It ends at the first red job.** The jobs still running are killed, no other
# job starts and the lint is skipped, so a push that broke a case is red when
# that job ends. With `BAR_PUSH_BASE` set to the commit a push moved the branch
# from, the jobs of the packages that range touched run first; the same jobs run
# in another order and none is dropped.
report: ## The report bar: check less the slow jobs the history lets go
	CARGO=$(CARGO) $(CARGO) run -q --locked -p jojobot-bar -- report

# **The size check alone**, the first phase of `check`, `report` and `dev`.
# `size-ratchet.toml` lists every tracked `.rs` file over the cap with its line
# count: a listed file may not grow, an unlisted file over the cap is refused,
# and the only way past is a raised entry that names its card. `make size` checks
# the tree. `make size SIZE_ARGS=--write` lowers or deletes the stale entries
# after a file shrank, and adds nothing. The base half compares with the list as
# it stood at `SIZE_BASE` (a revision), else `GITHUB_BASE_REF`; with neither it
# says unknown, and the tree half still decides.
SIZE_ARGS ?=
size: ## The size ratchet: no new file over the cap, no listed file grows
	CARGO=$(CARGO) $(CARGO) run -q --locked -p jojobot-bar -- size $(SIZE_ARGS)

# **The files of every text a model reads.** A tool's description, an argument's
# description, the instructions, the orientation essay, a skill, a teaching and
# the refusal of each error kind are rendered into `crates/jojobot-mcp/served-texts/`,
# one file to a surface, and `make check` fails when the files and the texts
# differ. A commit that changes one of those texts runs this and commits the
# rewritten files with it, so the diff of what models read is in the commit and
# is accepted by name. It rewrites the files and judges nothing.
snapshot: ## Rewrite the checked-in files of every text a model reads
	SNAPSHOT_UPDATE=1 $(MAKE) narrow CRATE=jojobot-mcp FILTER=every_text_a_model_reads_matches_the_snapshot

# **The dev bar.** The everyday loop, a level below the report: the format check,
# a build, then the unit tests and the fast test files of the packages the
# branch touches. The touched packages are the ones holding a path that differs
# from the merge base with `main` (committed, uncommitted or untracked). **The
# packages that depend on them are not tested here**: their tests are the branch
# run's, and the verdict names them and the run to read. A test file counts as
# slow on the history's word, as in the report level, and a file with no
# history runs. It then lints those same packages, whatever the tests said. It
# runs no room and no doc-test, and its verdict names every file and package
# it left out. **A branch is measured against `main` unless `BASE` names another
# ref**: `make dev BASE=forgejo/em-93-base` for a branch built on a release line,
# whose own commits `main` does not hold. **Its last line is its whole wall time**, and a run over five
# minutes says to push and let CI run it; a cold target directory may take
# longer, once.
#
# 🚨 **It is not a boundary run.** A defect that exists only where two packages
# meet, or in a file it skipped, is invisible to it. `make check` is the bar
# before a report or a carry. A diff that touches no package is refused.
dev: ## The dev bar: format, build, and the tests and lint of what the branch touched
	CARGO=$(CARGO) $(if $(BASE),BAR_DEV_BASE=$(BASE),) $(CARGO) run -q --locked -p jojobot-bar -- dev

# **The inner loop, scoped to one crate.** Run it between edits.
#
# It checks the format, then runs that crate's tests, then lints that crate.
# Those are the three ways a change inside one crate breaks on its own.
#
# **It refuses when no crate is named.** A scoped target that widened to the
# workspace by itself would make the narrow command and the full bar the same
# command, and a person reading either one could not tell which had run.
#
# **The format check covers the workspace and takes under a second.** It is
# here because it is the one class a crate-scoped run cannot see: no
# `cargo test -p` reads formatting, so that failure would wait for the
# boundary.
#
# ⚠️ It is not a replacement for `make check`. A defect that exists only where
# two crates meet is invisible to every scoped run, and that class is what the
# full bar is kept for.
#
# 🚨 **A run that selected nothing is refused, rather than reported as a pass.**
# `cargo test` exits zero over an empty selection, so `0 passed; 0 failed` and a
# green exit code are what a mistyped filter looks like — and the mistake that
# produces it is ordinary: FILTER is a SUBSTRING of a test's full path, so a
# regex written there matches nothing at all.
#
# **So the count is read before the run, and the count decides.** `--list` says
# what the filter selects; none is a caller error and never a legitimate green.
# The check reads a number rather than an exit code, which is the same rule it
# exists to enforce.
#
# ⚠️ **The refusal names the filter when there was one and the crate when there
# was not**, because a mistyped filter and a crate nobody has written tests for
# yet send a reader to different places. **Nothing in this workspace is in the
# second state today**, so that half is for the crate somebody adds next.
#
# 🚨 **A scoped run does not build what a test SPAWNS**, and one crate's tests
# spawn the server as a process rather than calling it. `cargo test -p
# jojobot-exercise` compiles its own cases and drives whatever `jojobot` binary
# was last built, at any commit — so it can report on code nobody under test is
# running. **This target builds the workspace first for that reason**, which
# costs an incremental link and removes the whole class. The rooms also refuse
# a binary older than the sources it is built from, so the two halves agree
# rather than one covering for the other.
#
#     make narrow CRATE=<name> [FILTER=<substring>]
#
# **Runs through `jojobot-bar` too**, for the same reason `check` does: one
# short verdict and the full output at a known path, same suites, same
# flags, same zero-selection guard, only what is printed changes.
#
# **FILTER may hold several words**, each a substring of a test path, so the
# cases a hand-off names run in one call: `FILTER="a_case another_case"`. **The
# last line is the run's whole wall time**, as `make dev`'s is.
CRATE ?=
FILTER ?=
narrow: ## The inner loop: one crate's tests and lint, plus the format check
	@test -n "$(CRATE)" || { echo "make narrow needs a crate: make narrow CRATE=<name>"; exit 2; }
	CARGO=$(CARGO) $(CARGO) run -q --locked -p jojobot-bar -- narrow --crate $(CRATE) $(if $(FILTER),--filter "$(FILTER)",)

# 🚨 **Every target reports, and the count is what a reader takes from it.**
#
# `cargo test` stops after the first failing target. So a run with an early
# failure prints a suite count that is a true statement about what RAN and a
# false impression of what was CHECKED, and nothing on screen says which. Three
# runs on this tree in one day reported 19, 16 and 16 suites ok out of
# forty-one, each of them looking like a mostly-green bar.
#
# **A partial red bar is worse than a plain one**, because the reader believes
# they know which parts are fine. It has already cost a hand-off: an implementer
# was told three failures were not theirs and to carry on, when twenty-two
# suites had not run at all.
#
# ⚠️ **It costs wall-clock on a red run** — the suites after the failure run
# instead of being skipped — and this is the BOUNDARY bar, taken before a
# report, a carry or a review. Paying for the whole answer is the trade the
# inner loop exists to keep you from paying between edits.
#
# ⚠️ **A target that fails to COMPILE still stops the run.** Compilation is not
# a test failure. That run reports no suites rather than a plausible count,
# which is the same defect arriving in a shape a reader cannot misread.
# **`--locked` asks the question the deploy build already asks.** The nix
# package builds with `cargoLock.lockFile = ./Cargo.lock`, which can neither
# rewrite the lock nor reach the network — so a manifest and a lock that
# disagree fail there. Without this flag, `cargo` repairs the disagreement
# silently and the bar never sees it: this is the boundary bar, so it asks
# the same question the artifact it gates is built from.
test: ## Every fast suite (no network)
	$(CARGO) test --workspace --no-fail-fast --locked

lint: ## Clippy, warnings fatal
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings

fmt: ## Reformat the workspace
	$(CARGO) fmt --all

fmt-check: ## Assert the workspace is formatted, rewriting nothing
	$(CARGO) fmt --all --check

build: ## Build the workspace
	$(CARGO) build --workspace --locked

# **The real-dependency gate, and it needs no credentials any more.**
#
# Every store jojobot fronts is a process it spawns itself, so the suites that
# run against a real one need a temporary directory and the binary already in
# the toolchain — which is why they are ordinary `cargo test` cases rather than
# an ignored tier somebody has to remember. `make check` runs them.
#
# This target stays as the name a person reaches for, and it runs the same
# thing rather than pretending there is a second gate.
integration: ## Run the suites against the real store
	$(CARGO) test -p jojobot-adapters --test dolt_store --test dolt_contract_parts --test dolt_without_a_home

# **The room suites, which `make check` does not run.** Each stands up a room:
# the shipped server and a store of its own. Which test files are room suites
# is declared once, in `crates/jojobot-exercise/Cargo.toml`, as the targets
# that need the `rooms` feature. Nothing here names one.
#
# `rooms` runs every room suite except the year room. Run it before a report
# when a slice touches a room's own files — `rooms/<name>.md`, its test file —
# or the harness in `crates/jojobot-exercise/src`.
#
# 🚨 **Builds the workspace first.** A room spawns the built `jojobot` binary,
# and `cargo test -p jojobot-exercise` does not rebuild it. Without the build
# an edit to served code is checked against the old binary.
#
# **The rooms run side by side**, through the bar, under the same `BAR_JOBS`
# limit as `make check` (eight by default). Every room runs and none stops
# another; the whole log is `target/bar/rooms.log`.
# **Each room target is named with `--test`, read out of the manifest.** The
# feature alone would also run the harness's unit tests and every test file
# that is not a room suite, which `make check` already runs.
ROOM_TESTS := $(shell awk '/^name = /{n=$$3; gsub(/"/,"",n)} /^required-features = \["rooms"\]/{print n}' crates/jojobot-exercise/Cargo.toml)
rooms: ## Every room suite except the year room
	CARGO=$(CARGO) $(AT_ROOT)$(BAR) rooms $(addprefix --test ,$(ROOM_TESTS))

# **Compile every test binary a pull request runs, once.** The bundle is
# `target/bundle.tar.gz`: the check, rooms and year lanes, each under its own
# features, with the server and the manifest. A job that unpacks it at the
# checkout root and sets `BAR_BUNDLE=target/bundle/manifest.json` runs `make
# check`, `make rooms` or `make year` on those binaries and compiles no test.
bundle: ## Compile the pull-request bar's test binaries into target/bundle.tar.gz
	JOJOBOT_CI_CACHE="$$CARGO_TARGET_DIR" CARGO=$(CARGO) $(AT_ROOT)$(CARGO) run -q --locked -p jojobot-bar -- bundle $(addprefix --room ,$(ROOM_TESTS))

# **One room's suite**, by the name of its test file without the `_room`
# ending: `make room ROOM=year`, `make room ROOM=vault`. A file without that
# ending is named whole, `make room ROOM=piano_lock`. It turns every feature
# on, so it also reaches the year room.
# **A retired room is refused by name.** The manifest lists which rooms run, and
# a room behind the `retired` feature is a replay kept as evidence of an older
# product: `rooms` leaves it out, and so does this target, because `--all-features`
# would run it as if it were live and report a red nobody can act on.
RETIRED_TESTS := $(shell awk '/^name = /{n=$$3; gsub(/"/,"",n)} /^required-features = \["retired"\]/{print n}' crates/jojobot-exercise/Cargo.toml)
ROOM ?=
ROOM_TEST = $(if $(wildcard crates/jojobot-exercise/tests/$(ROOM)_room.rs),$(ROOM)_room,$(ROOM))
room: ## One room's suite: make room ROOM=<name>
	@test -n "$(ROOM)" || { echo "make room needs a room: make room ROOM=<name>"; exit 2; }
	@test -f crates/jojobot-exercise/tests/$(ROOM_TEST).rs || { echo "no test file named $(ROOM)_room or $(ROOM) under crates/jojobot-exercise/tests"; exit 2; }
	@case " $(RETIRED_TESTS) " in *" $(ROOM_TEST) "*) echo "$(ROOM_TEST) is a retired room: a replay of a run from before own fields stayed on their record, kept as evidence and run by no target"; exit 2;; esac
	CARGO=$(CARGO) $(AT_ROOT)$(BAR) rooms --lane $(ROOM) --all-features --test $(ROOM_TEST)

# **The year room, alone.** It is the slowest suite and no other target runs
# it: `rooms` leaves it out and `check` runs no room. `prepush` calls it.
year: ## The year room's suite
	$(MAKE) room ROOM=year

nix-build: ## The package build, as release QA runs it
	nix build

# `-L` prints each build's log as it runs, and a build that prints nothing for
# half an hour fails instead of holding a runner in silence.
flake-check: ## The flake's checks, as release QA runs them
	nix flake check -L --max-silent-time 1800

# **Release QA, run before a push.** The whole bar and the real store, every
# room suite, the year room (which no other target runs), the package build
# and the flake check.
#
# **Every step runs and each one reports**, because a red first step must not
# hide whether the rest are green. `-k` is what keeps going past a red step,
# and the exit code is non-zero if any step failed. The steps are targets of
# their own so a dry run (`make -n prepush`) lists them and runs none of them.
#
# **The flake check runs beside the bar steps, not after them.** It builds on a
# remote builder and takes most of an hour, and nothing in the bar steps feeds
# it, so it starts first and the bar steps run while it builds. The bar steps
# stay serial among themselves (`-j1`): they share the runner and its target
# directory. `-j2` is what lets the two lines run together, and nothing else.
prepush: ## Before a push: check, integration, rooms, year room (skipped when the forge holds this commit's PR proof), nix build, flake check
	$(MAKE) -k -j2 prepush-steps

.PHONY: prepush-steps bar-steps year nix-build flake-check
prepush-steps: flake-check bar-steps

bar-steps:
	$(MAKE) -j1 -k bar-arm nix-build

# **The serial bar arm runs unless the forge already holds this commit's proof.**
# scripts/pr-proof exits 0 only for a successful pull-request run of exactly HEAD
# with a green job for every step of the arm, and prints the run it relied on; every other answer, a
# forge that does not answer included, leaves the arm to run in full. The verdict
# goes through a stamp file so the sub-make line holds no lookup: a dry run
# (`make -n`) prints the arm and never asks the forge.
#
# **BAR_ARM is the one list of the arm's steps, and scripts/pr-proof reads it.** A
# pull-request run vouches only when it holds a green job of each name here, so the
# proof can never cover less than the arm it replaces. Each name is a make target
# and the pull-request job that runs it. `integration` is not on the list: no
# pull-request job runs it, because `make check` runs the same suites.
BAR_ARM = check rooms year
PR_PROOF_STAMP = target/bar/pr-proof.stamp
.PHONY: bar-arm
bar-arm:
	@rm -f $(PR_PROOF_STAMP); if scripts/pr-proof; then mkdir -p $(dir $(PR_PROOF_STAMP)) && touch $(PR_PROOF_STAMP); fi
	@test -f $(PR_PROOF_STAMP) || $(MAKE) -j1 -k $(BAR_ARM)

# **The report `cargo test` swallows.** A passing test's stdout is captured
# and thrown away, so served_arguments_with_no_story_site_are_reported
# (crates/jojobot/tests/argument_coverage.rs) never reaches a reader through
# `make check` or a plain `cargo test` — a lead nobody can see is the failure
# this report exists to end. This target is how a person actually reads it.
argument-coverage: ## Print every served argument the story suite never calls with
	$(CARGO) test -p jojobot --test argument_coverage served_arguments_with_no_story_site_are_reported -- --nocapture

# **The third tier, and it is billed.**
#
# The suites above ask whether the code works. This one drives a REAL model
# through the surface as shipped, against an instance built from nothing, and
# asserts over what is in the store afterwards — the one question a scripted
# client cannot ask, because a scripted client is told which verb to call.
#
# It is named for what it costs. It reaches the network, it spends money, and
# `make check` must never run it: nothing here is a `cargo test` case, so it
# happens when somebody types it and at no other time.
#
# **The run is kept.** What a paid run is judged by is the part no assertion
# touches — what the model reached for, what it did not find, what it concluded
# — and that lived on stdout and nowhere else. It is written whole, under
# `transcripts/`, and the run says where it went. TRANSCRIPT=<path> puts it
# somewhere else.
#
#     make paid [PLAYBOOK=<path>] [MODEL=<name>] [TRANSCRIPT=<path>]
PLAYBOOK ?=
MODEL ?=
TRANSCRIPT ?=
paid: build ## Drive a REAL model through a playbook — reaches the network and COSTS MONEY
	$(CARGO) run -q --locked -p jojobot-exercise -- \
		$(if $(PLAYBOOK),--playbook $(PLAYBOOK),) $(if $(MODEL),--model $(MODEL),) \
		$(if $(TRANSCRIPT),--transcript $(TRANSCRIPT),)

# **Send a paid run's recorded calls to this build, and list every call whose
# class changed.** No model runs and nothing is billed. It reports and never
# fails: an intended fix changes a class too, and the deploy's notes accept it by
# name. It runs after `make rooms` at a deploy cut, against the newest stream of
# each paid room, kept outside the repository (REPLAY_BASELINES, else the state
# directory). `JOJOBOT_BIN` names another build as the candidate.
#
#     make replay [STREAM=<path>] [ROOM=<name>] [CANDIDATE=<label>]
STREAM ?=
CANDIDATE ?=
replay: build ## Replay a paid run's recorded calls on this build; reports the calls whose class changed
	$(CARGO) run -q --locked -p jojobot-exercise --bin replay -- \
		$(if $(STREAM),--stream $(STREAM),) $(if $(ROOM),--room $(ROOM),) \
		$(if $(CANDIDATE),--candidate $(CANDIDATE),)

# **Refreshing the upgrade fixture is one command.** It builds a binary at a
# named ref in a DETACHED worktree — this checkout's own HEAD never moves and
# no branch is created — boots it on a fresh disposable store, seeds a
# representative set of records through its served surface, and dumps the
# store to a fixture directory under crates/jojobot/tests/fixtures/, with the ref
# recorded beside it. The upgrade gate itself (make check) reads that committed
# fixture; this target is what a deploy runs to refresh it, never a test.
#
#     make refresh-upgrade-fixture [REF=<ref>] DIR=<fixture directory>
#
# DIR is relative to crates/jojobot and has no default; the target refuses
# without it. The gate keeps the fixtures of the deployed builds it proves an
# upgrade from, for the build the operator's third deploy shipped (its commit is
# in that fixture's ref.txt) in DIR=tests/fixtures/upgrade_deploy3, for the build
# the fourth deploy shipped in DIR=tests/fixtures/upgrade_deploy4 and for the
# build the fifth deploy shipped in DIR=tests/fixtures/upgrade_deploy5 and for the
# build deploy 6.1 shipped in DIR=tests/fixtures/upgrade_deploy6 and for the
# build deploy 8 shipped in DIR=tests/fixtures/upgrade_deploy8 and for the
# build deploy 8.1 shipped in DIR=tests/fixtures/upgrade_deploy8_1 and for the
# build deploy 9.1 shipped in DIR=tests/fixtures/upgrade_deploy9_1. The
# recording needs a build that takes `sets` and writes a closing entry on a wrap.
REF ?= origin/main
DIR ?=
refresh-upgrade-fixture: ## Record the upgrade gate's fixture from a binary built at REF (default origin/main)
	scripts/refresh-upgrade-fixture $(REF) $(DIR)

# **What a carry would move**, read from the repository and nothing else: this
# checkout's commits that main lacks, main's commits that origin/main lacks (the
# next push), and whether main has moved on since this checkout left it. It runs
# no fetch, so origin/main is whatever the repository already holds.
#
#     make carried
carried: ## What this branch holds past main, what main holds past origin/main, and whether main moved on
	@scripts/carried
