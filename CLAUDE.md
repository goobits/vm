# Claude Development Notes

## Installation

### From Cargo (Recommended)
```bash
cargo install vm
```

### From Source
```bash
git clone <repository-url>
cd vm
./install.sh
```

The `install.sh` script builds the binary and sets up symlinks automatically.

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
cargo test --package vm-provider
cargo test --package vm-installer
cargo test --package vm-temp
cargo test --package vm
cargo test --package vm-common
cargo test --package vm-pkg

# Test specific modules within packages
cargo test --package vm-config config_ops_tests
cargo test --package vm workflow_tests
cargo test --package vm-config detector::tests::nodejs_tests

# Test VM operations integration tests (requires Docker)
cargo test --package vm --test vm_operations_integration_tests
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
cargo test --workspace -- --list
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
- `vm/tests/vm_operations_integration_tests.rs` - **VM operations integration tests (NEW)**
- `vm/tests/config_cli_tests.rs` - CLI configuration command testing
- `rust/tests/integration_tests.rs` - Cross-crate integration scenarios
- `vm-config/src/detector/tests/` - Framework detection tests

### VM Operations Integration Tests (NEW)

The `vm/tests/vm_operations_integration_tests.rs` file provides comprehensive testing for all core VM commands:

**Commands Tested:**
- `create` - VM creation with and without --force flag
- `start` - Starting VMs and verifying running state
- `stop` - Stopping VMs and verifying stopped state
- `restart` - Restarting VMs and state transitions
- `provision` - Re-running provisioning on existing VMs
- `list` - Listing all VMs
- `kill` - Force killing VM processes
- `destroy` - Destroying VMs and cleanup verification
- `ssh` - SSH connection handling
- `status` - VM status reporting
- `exec` - Command execution inside VMs
- `logs` - VM log retrieval

**Test Features:**
- Uses real Docker provider (no mocks)
- Runs in isolated temporary directories (`/tmp/`)
- Safe cleanup of test containers
- Unique project names to avoid conflicts
- Graceful skipping when Docker unavailable
- Full lifecycle integration testing

```bash
# Run all VM operations tests
cargo test --package vm --test vm_operations_integration_tests

# Run specific VM operation test
cargo test --package vm test_vm_create_command

# Run with output (helpful for debugging Docker issues)
cargo test --package vm --test vm_operations_integration_tests -- --nocapture
```

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
source $HOME/.cargo/env
cd rust

# Check for dead code using Clippy
cargo clippy --workspace -- -D dead_code

# Check for unused dependencies
cargo dead-deps
```

### Tools
- **Clippy** (built-in): Detects unused code, imports, variables, unreachable code
- **cargo-machete**: Detects unused dependencies in Cargo.toml files

### Configuration
- **Cargo aliases**: `.cargo/config.toml` in rust directory - Contains dead-code detection commands
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


## Test Structure

The Rust test suite uses dynamic fixture generation rather than static test files:
- Tests create temporary directories and files as needed
- Each crate manages its own test fixtures independently
- The `vm-config` crate includes a `ProjectTestFixture` struct for test setup
- Integration tests use `CrossCrateTestFixture` for cross-crate testing

Example configuration files for users are available in the `examples/` directory.