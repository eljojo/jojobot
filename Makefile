# jojobot — the green bar, written down once.
#
# Run these THROUGH the flake, which is where the toolchain lives:
#
#     nix develop -c make check
#
# The recipes call `cargo` directly rather than shelling out to `nix develop`
# themselves, so running `make` from inside the dev shell doesn't nest a second
# one. That is also why `make` itself is in the shell's inputs.

CARGO ?= cargo

.DEFAULT_GOAL := help
.PHONY: help check narrow test lint fmt fmt-check build integration argument-coverage paid refresh-upgrade-fixture

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
check: ## The DONE bar: formatted, green, clippy-clean
	CARGO=$(CARGO) $(CARGO) run -q -p jojobot-bar -- check

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
CRATE ?=
FILTER ?=
narrow: ## The inner loop: one crate's tests and lint, plus the format check
	@test -n "$(CRATE)" || { echo "make narrow needs a crate: make narrow CRATE=<name>"; exit 2; }
	CARGO=$(CARGO) $(CARGO) run -q -p jojobot-bar -- narrow --crate $(CRATE) $(if $(FILTER),--filter $(FILTER),)

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
	$(CARGO) build --workspace

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
	$(CARGO) test -p jojobot-adapters --test dolt_store --test dolt_without_a_home

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
	$(CARGO) run -q -p jojobot-exercise -- \
		$(if $(PLAYBOOK),--playbook $(PLAYBOOK),) $(if $(MODEL),--model $(MODEL),) \
		$(if $(TRANSCRIPT),--transcript $(TRANSCRIPT),)

# **Refreshing the upgrade fixture is one command.** It builds a binary at a
# named ref in a DETACHED worktree — this checkout's own HEAD never moves and
# no branch is created — boots it on a fresh disposable store, seeds a
# representative set of records through its served surface, and dumps the
# store to crates/jojobot/tests/fixtures/upgrade/, with the ref recorded
# beside it. The upgrade gate itself (make check) reads that committed
# fixture; this target is what a deploy runs to refresh it, never a test.
#
#     make refresh-upgrade-fixture [REF=<ref>]
REF ?= origin/main
refresh-upgrade-fixture: ## Record the upgrade gate's fixture from a binary built at REF (default origin/main)
	scripts/refresh-upgrade-fixture $(REF)
