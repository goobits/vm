.PHONY: help build build-no-bump test clippy fmt check-duplicates check bump-version

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
	@echo "  make check            - Run fmt + clippy + test"
	@echo "  make check-duplicates - Check for code duplication"
	@echo ""

# Build (with automatic version bump)
build:
	@./scripts/bump-version.sh
	cargo build --workspace

# Build without version bump
build-no-bump:
	cargo build --workspace

# Test
test:
	cargo test --workspace

# Code quality
clippy:
	cargo clippy --workspace

fmt:
	cargo fmt --all

# Analysis
check-duplicates:
	./scripts/check-duplicates.sh

# Version management
bump-version:
	@./scripts/bump-version.sh

# Common combo
check: fmt clippy test
