.PHONY: build build-release build-semantic run run-release test check fmt clippy clean help

# Default target
all: check build

## Build targets
build: ## Build in debug mode
	cargo build

build-release: ## Build in release mode
	cargo build --release

build-semantic: ## Build with semantic search (fastembed) enabled
	cargo build --features semantic

build-semantic-release: ## Release build with semantic search
	cargo build --release --features semantic

## Run targets
run: ## Run the MCP server (debug)
	cargo run

run-release: ## Run the MCP server (release)
	cargo run --release

run-semantic: ## Run with semantic search enabled
	cargo run --features semantic

## Quality targets
check: fmt clippy ## Run formatter and linter

fmt: ## Check formatting
	cargo fmt --check

fmt-fix: ## Auto-fix formatting
	cargo fmt

clippy: ## Run clippy lints
	cargo clippy -- -D warnings

clippy-fix: ## Auto-fix clippy warnings
	cargo clippy --fix --allow-dirty

test: ## Run all tests
	cargo test

test-verbose: ## Run tests with output
	cargo test -- --nocapture

## Maintenance targets
clean: ## Remove build artifacts
	cargo clean

doc: ## Generate documentation
	cargo doc --no-deps --open

update: ## Update dependencies
	cargo update

audit: ## Audit dependencies for vulnerabilities
	cargo audit

## Help
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-22s\033[0m %s\n", $$1, $$2}'
