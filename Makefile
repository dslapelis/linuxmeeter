# linuxmeeter — see CLAUDE.md / README.md for what these do and why.
# Every target is phony; the real build systems (pnpm, cargo) do the tracking.

PNPM  ?= pnpm
CARGO ?= cargo
# Engine logging for `make app`, `make graph`, `make spike`.
LOG   ?= info

.DEFAULT_GOAL := help

.PHONY: help install dev app check check-ui check-rust fmt fmt-check clippy \
        test test-rust test-ui test-audio test-all \
        build run graph spike clean clean-cache

help: ## List targets
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[1m%-12s\033[0m %s\n", $$1, $$2}'

install: ## Install frontend dependencies
	$(PNPM) install

# ---- running ---------------------------------------------------------------

dev: ## Browser-only UI against the mock backend (design iteration)
	$(PNPM) dev

app: ## Full app against the live PipeWire graph
	RUST_LOG=$(LOG) $(PNPM) tauri dev

run: ## Run the already-built release binary
	RUST_LOG=$(LOG) ./target/release/linuxmeeter

# ---- headless engine REPLs (no UI; drive real audio) ------------------------

graph: ## Headless full topology: route a1 0|1 | links | meters 1 | q
	RUST_LOG=$(LOG) $(CARGO) run --example graph -p lm-engine

spike: ## Headless single strip: set comp:cr 8 | setdb gate:gt -30 | vol 0.5 | q
	RUST_LOG=$(LOG) $(CARGO) run --example spike -p lm-engine

# ---- tests -----------------------------------------------------------------

test: test-rust test-ui ## Run all tests that need no audio system

test-rust: ## Rust unit tests (audio integration tests are skipped)
	$(CARGO) test -p lm-protocol -p lm-engine

test-ui: ## Frontend unit tests (vitest)
	$(PNPM) test

test-audio: ## Integration tests against a private throwaway PipeWire daemon
	./scripts/with-test-daemon.sh \
		$(CARGO) test -p lm-engine --test audio -- --ignored --test-threads=1

test-all: test test-audio ## Everything, including the audio integration tests

# ---- checks ----------------------------------------------------------------

check: check-ui check-rust ## Type-check frontend and Rust workspace

check-ui: ## svelte-check
	$(PNPM) check

check-rust: ## cargo check across the workspace
	$(CARGO) check --workspace --all-targets

clippy: ## cargo clippy across the workspace
	$(CARGO) clippy --workspace --all-targets

fmt: ## Format Rust sources
	$(CARGO) fmt --all

fmt-check: ## Verify Rust formatting without writing
	$(CARGO) fmt --all --check

# ---- building --------------------------------------------------------------

build: ## Release binary at target/release/linuxmeeter
	$(PNPM) tauri build --no-bundle

# ---- cleaning --------------------------------------------------------------

clean: ## Remove build artifacts (target/, dist/)
	$(CARGO) clean
	rm -rf dist

clean-cache: ## Wipe the WebKitGTK module cache (unstyled-UI fix)
	rm -rf ~/.local/share/com.stacksloth.linuxmeeter/WebKitCache
