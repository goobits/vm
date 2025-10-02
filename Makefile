.PHONY: build test clippy fmt check-duplicates check

# Build
build:
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

# Common combo
check: fmt clippy test
