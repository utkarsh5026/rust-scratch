# rust-scratch task runner. Run `make` (or `make help`) to see targets.
# Most days you just want `make run BIN=<concept>` or `make stats`.

.DEFAULT_GOAL := help
.PHONY: help run stats list fmt fmt-check clippy build check miri ci docs docs-build

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

run: ## Run a practice bin: make run BIN=cow
	@test -n "$(BIN)" || { echo "usage: make run BIN=<concept>  (see 'make list')"; exit 1; }
	cargo run --bin $(BIN)

stats: ## Show the gamified progress dashboard
	cargo run --bin stats

list: ## List available bins
	@scripts/miri --list

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting without writing (CI gate)
	cargo fmt --all -- --check

clippy: ## Lint with clippy (informational, matches CI)
	cargo clippy --workspace --all-targets

build: ## Build everything
	cargo build --workspace --all-targets

check: ## Fast type-check without producing binaries
	cargo check --workspace --all-targets

miri: ## Run a bin under Miri: make miri BIN=box_heap [LEAK=1]
	@test -n "$(BIN)" || { echo "usage: make miri BIN=<concept> [LEAK=1]"; exit 1; }
	scripts/miri $(if $(LEAK),--ignore-leaks) $(BIN)

docs: ## Serve the mdBook knowledge base with live reload + open browser
	mdbook serve docs --open

docs-build: ## Build the mdBook site once into docs/book
	mdbook build docs

ci: fmt-check clippy build ## Run the full CI suite locally
