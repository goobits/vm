.PHONY: help build build-no-bump test clippy fmt fmt-fix check-duplicates check bump-version quality-gates deny

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
	@echo "  make test             - Run all tests"
	@echo "  make clippy           - Run clippy linter"
	@echo "  make fmt              - Format all code"
	@echo "  make audit            - Check dependencies for security vulnerabilities"
	@echo "  make check            - Run fmt + clippy + test"
	@echo "  make quality-gates    - Run all quality checks (fmt + clippy + audit + test)"
	@echo "  make check-duplicates - Check for code duplication"
	@echo ""

# Build (with automatic version bump)
build:
	@./scripts/bump-version.sh
	cd rust && cargo build --workspace

# Build without version bump
build-no-bump:
	cd rust && cargo build --workspace

# Test
test:
	cd rust && cargo test --workspace

# Code quality
clippy:
	cd rust && cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cd rust && cargo fmt --all --check

fmt-fix:
	cd rust && cargo fmt --all

audit:
	cd rust && cargo audit

# Analysis
check-duplicates:
	./scripts/check-duplicates.sh

# Version management
bump-version:
	@./scripts/bump-version.sh

# Quality gates - run all checks before committing
quality-gates: fmt clippy audit test
	@echo ""
	@echo "✅ All quality gates passed!"

# Run formatting, linting, and tests
check: fmt-fix clippy test
