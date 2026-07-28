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
INTEGRATION_SUITES := outline_integration

.DEFAULT_GOAL := help
.PHONY: help check test lint fmt fmt-check build integration

help: ## List the targets
	@grep -hE '^[a-z-]+:.*##' $(MAKEFILE_LIST) \
		| sed -e 's/:.*## / — /' \
		| awk '{ printf "  \033[1m%-12s\033[0m %s\n", $$1, substr($$0, index($$0, "—")) }'
	@echo ""
	@echo "  integration needs credentials and touches real stores — see its rule."

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

# **The real-dependency gate.** A slice that touches an adapter does not merge on
# fakes alone. Credentials come from `.env` and are sourced into this recipe's
# shell only — never echoed, never passed on a command line where they would land
# in a process list.
#
# It fails loudly when `.env` is missing rather than skipping: the suites
# themselves panic on absent credentials for the same reason, because a gate that
# prints "skipping" and exits green is a run that verified nothing while reading
# as if it had.
integration: ## Run the suites against real stores (needs .env)
	@test -f .env || { \
		echo "no .env — the real-dependency suites need credentials, and a skipped run is not a green one"; \
		exit 1; \
	}
	@set -a; . ./.env; set +a; \
	for suite in $(INTEGRATION_SUITES); do \
		echo "== $$suite"; \
		$(CARGO) test -p jojobot-adapters --test $$suite -- --ignored || exit 1; \
	done
