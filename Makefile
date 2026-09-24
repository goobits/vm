.PHONY: help build build-no-bump test test-shell test-unit test-bin test-integration test-integration-conditional test-network clippy fmt fmt-fix check-duplicates check bump-version quality-gates deny watch dev udeps audit

CARGO_JOBS ?= 1
CARGO_TARGET_DIR ?= /tmp/vm-rust-target
export CARGO_TARGET_DIR
INTEGRATION_FEATURES := integration,vm-package-server/standalone-binary

# Default target - show help
.DEFAULT_GOAL := help

# Help target
help:
	@echo "Available targets:"
	@echo ""
	@echo "  make build            - Build with automatic version bump (+0.0.1)"
	@echo "  make build-no-bump    - Build without version bump"
	@echo "  make bump-version     - Bump version without building"
	@echo ""
	@echo "  make test             - Run all tests (unit + integration, no network)"
	@echo "  make test-shell       - Run shell regression tests"
	@echo "  make test-unit        - Run library unit tests"
	@echo "  make test-bin         - Run CLI binary unit tests"
	@echo "  make test-integration - Run integration tests"
	@echo "  make test-network     - Run network tests (requires TLS/Keychain access)"
	@echo "  make clippy           - Run clippy linter"
	@echo "  make fmt              - Format all code"
	@echo "  make audit            - Check dependency policy"
	@echo "  make udeps            - Find unused dependencies"
	@echo "  make check            - Run fmt + clippy + test"
	@echo "  make quality-gates    - Run CI quality checks (fmt + clippy + audit + duplicates + test)"
	@echo "  make check-duplicates - Check for code duplication"
	@echo ""
	@echo "  make watch            - Watch for changes and run tests (cargo-watch)"
	@echo "  make dev              - Watch for changes and run checks (cargo-watch)"
	@echo ""
	@echo "  CARGO_JOBS=N          - Override Cargo parallelism (default: 1)"
	@echo ""

# Build (with automatic version bump)
build:
	@./scripts/dev/bump-version.sh
	cd rust && cargo build --workspace -j $(CARGO_JOBS)

# Build without version bump
build-no-bump:
	cd rust && cargo build --workspace -j $(CARGO_JOBS)

# Test (using nextest for faster execution)
test: test-shell test-unit test-bin test-integration-conditional

test-shell:
	@./scripts/internal/test-install-checksum.sh
	@./scripts/internal/test-vm-worktree.sh

test-unit:
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cd rust && cargo nextest run --workspace --lib --test-threads=10; \
	else \
		cd rust && cargo test --workspace --lib -j $(CARGO_JOBS) -- --test-threads=10; \
	fi

test-bin:
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cd rust && cargo nextest run -p goobits-vm --bin vm --test-threads=10; \
	else \
		cd rust && cargo test -p goobits-vm --bin vm -j $(CARGO_JOBS) -- --test-threads=10; \
	fi

test-integration:
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cd rust && cargo nextest run --workspace --test '*' --features $(INTEGRATION_FEATURES) --test-threads=2; \
	else \
		cd rust && cargo test --workspace --test '*' --features $(INTEGRATION_FEATURES) -j $(CARGO_JOBS) -- --test-threads=2; \
	fi

test-integration-conditional:
ifndef SKIP_INTEGRATION_TESTS
	$(MAKE) test-integration
endif

test-network:
	@echo "⚠️  Network tests require TLS certificates and may prompt for Keychain access"
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cd rust && cargo nextest run --workspace --features network-tests --test-threads=2; \
	else \
		cd rust && cargo test --workspace --features network-tests -j $(CARGO_JOBS) -- --test-threads=2; \
	fi

# Code quality
clippy:
	cd rust && cargo clippy --workspace --all-targets --all-features -j $(CARGO_JOBS) -- -D warnings

fmt:
	cd rust && cargo fmt --all --check

fmt-fix:
	cd rust && cargo fmt --all

audit:
	cd rust && cargo deny check

# Find unused dependencies
udeps:
	@if command -v cargo-udeps >/dev/null 2>&1; then \
		cd rust && cargo +nightly udeps --workspace; \
	else \
		echo "❌ cargo-udeps not installed. Install with: cargo install cargo-udeps"; exit 1; \
	fi

# Analysis
check-duplicates:
	./scripts/dev/check-duplicates.sh

# Version management
bump-version:
	@./scripts/dev/bump-version.sh

# Quality gates - run all checks before committing
quality-gates: fmt clippy audit check-duplicates test
	@echo ""
	@echo "✅ All quality gates passed!"

# Run formatting, linting, and tests
check: fmt clippy test

# Development watchers (requires cargo-watch)
watch:
	@if command -v cargo-watch >/dev/null 2>&1; then \
		cd rust && cargo watch -x 'nextest run'; \
	else \
		echo "❌ cargo-watch not installed. Install with: cargo install cargo-watch"; exit 1; \
	fi

dev:
	@if command -v cargo-watch >/dev/null 2>&1; then \
		cd rust && cargo watch -x fmt -x 'clippy --workspace --all-targets --all-features' -x 'nextest run'; \
	else \
		echo "❌ cargo-watch not installed. Install with: cargo install cargo-watch"; exit 1; \
	fi
