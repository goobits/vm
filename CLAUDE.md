# Claude Development Notes

## Running Tests

### Full Test Suite
```bash
source $HOME/.cargo/env
cd rust
cargo test --workspace
```

### Individual Package Tests
```bash
# Test specific packages
cargo test --package vm-config
cargo test --package vm-detector
cargo test --package vm-ports
cargo test --package vm-provider
cargo test --package vm-installer
cargo test --package vm-temp
cargo test --package vm
cargo test --package vm-common
cargo test --package vm-pkg

# Test specific modules within packages
cargo test --package vm-config config_ops_tests
cargo test --package vm workflow_tests
cargo test --package vm-detector tests::nodejs_tests
```

### Integration vs Unit Tests
```bash
# Integration tests (cross-crate functionality)
cargo test integration_tests

# Unit tests only (exclude integration)
cargo test --lib

# Doc tests only
cargo test --doc
```

### Test Output and Debugging
```bash
# Show test output (including println!)
cargo test -- --nocapture

# Run tests with backtraces on failure
RUST_BACKTRACE=1 cargo test

# Run single test by name
cargo test test_basic_config_workflow

# Run tests matching pattern
cargo test config_

# List all tests without running
cargo test -- --list
```

### Parallel vs Sequential Testing
```bash
# Run tests in sequence (helpful for debugging race conditions)
cargo test -- --test-threads=1

# Default parallel execution
cargo test
```

## Test Structure Overview

### Test Categories
- **Unit Tests**: In `src/` files with `#[cfg(test)]` modules
- **Integration Tests**: In `tests/` directories, test cross-crate functionality
- **Doc Tests**: Examples in doc comments, verify API usage

### Key Test Files
- `vm-config/tests/config_ops_tests.rs` - Configuration operations (uses TEST_MUTEX)
- `vm/tests/workflow_tests.rs` - CLI end-to-end workflows (uses TEST_MUTEX)
- `vm/tests/temp_workflow_tests.rs` - Temp VM lifecycle testing
- `rust/tests/integration_tests.rs` - Cross-crate integration scenarios
- `vm-detector/src/tests/` - Framework detection tests (9 modules)
- `vm-ports/src/` - Port range and registry tests (embedded in source)

### Test Synchronization
Environment-modifying tests use `TEST_MUTEX` to prevent race conditions:
```rust
use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn my_test() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    // Test code that modifies HOME, VM_TOOL_DIR, etc.
}
```

## Build Commands

```bash
# Check compilation without building
cargo check --workspace

# Build all packages (debug)
cargo build --workspace

# Build in release mode
cargo build --workspace --release

# Clean build artifacts
cargo clean

# Update dependencies
cargo update
```

## Troubleshooting

### Common Issues
- **Compilation errors**: Run `cargo check --package <name>` to isolate
- **Test failures**: Use `-- --nocapture` and `RUST_BACKTRACE=1` for debugging
- **Race conditions**: Use `-- --test-threads=1` to run tests sequentially
- **Missing cargo**: Ensure Rust toolchain is installed and in PATH

### Environment Setup
```bash
# Install Rust if missing
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verify installation
cargo --version
rustc --version
```

## Dead Code Detection

Run dead code detection with:
```bash
source /home/developer/.cargo/env
cd rust

# Check for dead code (unused functions, structs, etc.)
cargo dead-code

# Quick dead code check with error on findings
cargo dead-code-quick

# Check for unused dependencies
cargo dead-deps

# Comprehensive analysis (both code and dependencies)
cargo dead-all
```

### Tools
- **Clippy** (built-in): Detects unused code, imports, variables, unreachable code
- **cargo-machete**: Detects unused dependencies in Cargo.toml files

### Configuration
- **Cargo aliases**: `.cargo/config.toml` - Contains dead-code detection commands
- **Clippy config**: `clippy.toml` - Additional linting rules for code quality

### Lint Categories
The dead-code detection checks for:
- `dead_code` - Unused functions, structs, enums, methods
- `unused_imports` - Import statements that aren't used
- `unused_variables` - Variables that are never read
- `unused_mut` - Mutable variables that don't need to be mutable
- `unreachable_code` - Code that can never be executed
- `unreachable_patterns` - Match patterns that can never be reached
- `clippy::redundant_clone` - Unnecessary cloning operations
- `clippy::unnecessary_wraps` - Functions that always return Ok/Some
- `clippy::unused_self` - Methods that don't use self
- `clippy::unused_async` - Async functions that don't need to be async

### Fixing Dead Code
1. **Review findings carefully** - Some code may be intentionally kept for future use
2. **For false positives** - Add `#[allow(dead_code)]` to specific items
3. **For unused dependencies** - Run `cargo machete --fix` to auto-remove
4. **For test/example code** - These are often false positives and can be ignored