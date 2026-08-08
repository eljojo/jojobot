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
.PHONY: help check test lint fmt fmt-check build integration

help: ## List the targets
	@grep -hE '^[a-z-]+:.*##' $(MAKEFILE_LIST) \
		| sed -e 's/:.*## / — /' \
		| awk '{ printf "  \033[1m%-12s\033[0m %s\n", $$1, substr($$0, index($$0, "—")) }'

check: fmt-check test lint ## The DONE bar: formatted, green, clippy-clean

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
