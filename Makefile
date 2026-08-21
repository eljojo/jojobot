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
#     make narrow CRATE=<name>
CRATE ?=
narrow: ## The inner loop: one crate's tests and lint, plus the format check
	@test -n "$(CRATE)" || { echo "make narrow needs a crate: make narrow CRATE=<name>"; exit 2; }
	$(CARGO) fmt --all --check
	$(CARGO) test -p $(CRATE)
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
