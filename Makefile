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
.PHONY: help check narrow test lint fmt fmt-check build integration paid

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
check: fmt-check test lint ## The DONE bar: formatted, green, clippy-clean

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
CRATE ?=
FILTER ?=
narrow: ## The inner loop: one crate's tests and lint, plus the format check
	@test -n "$(CRATE)" || { echo "make narrow needs a crate: make narrow CRATE=<name>"; exit 2; }
	$(CARGO) fmt --all --check
	$(CARGO) build --workspace
	@n=$$($(CARGO) test -p $(CRATE) $(FILTER) -- --list 2>/dev/null | grep -c ': test$$' || true); \
	test "$$n" -gt 0 || { \
		if [ -n "$(FILTER)" ]; then \
			echo "make narrow: FILTER='$(FILTER)' selected no tests in $(CRATE), so the run would have reported a pass over nothing."; \
			echo "FILTER is a SUBSTRING of a test's full path, not a regex. Name one substring, or drop FILTER to run the whole crate."; \
		else \
			echo "make narrow: $(CRATE) has no tests, so the run would have reported a pass over nothing."; \
		fi; \
		exit 2; \
	}
	$(CARGO) test -p $(CRATE) $(FILTER)
	$(CARGO) clippy -p $(CRATE) --all-targets -- -D warnings

test: ## Every fast suite (no network)
	$(CARGO) test --workspace

lint: ## Clippy, warnings fatal
	$(CARGO) clippy --workspace --all-targets -- -D warnings

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
#     make paid PLAYBOOK=<path> [MODEL=<name>] [TRANSCRIPT=<path>]
PLAYBOOK ?=
MODEL ?=
TRANSCRIPT ?=
paid: build ## Drive a REAL model through a playbook — reaches the network and COSTS MONEY
	@test -n "$(PLAYBOOK)" || { echo "make paid needs a playbook: make paid PLAYBOOK=<path>"; exit 2; }
	$(CARGO) run -q -p jojobot-exercise -- \
		--playbook $(PLAYBOOK) $(if $(MODEL),--model $(MODEL),) \
		$(if $(TRANSCRIPT),--transcript $(TRANSCRIPT),)
