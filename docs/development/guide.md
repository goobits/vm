# Claude Development Notes

VM Tool is a Rust-based development environment manager that provides isolated, reproducible environments via Dockerpodman, or Tart. This guide covers building from source, testing, and the development workflow.

**Before you start:**
- Review the [Architecture Guide](architecture.md) to understand system design
- Check [Testing Guide](testing.md) for test strategy and best practices
- Join discussions in GitHub Issues for feature planning

## Installation

### From Source (Recommended)
```bash
git clone https://github.com/goobits/vm.git
cd vm
./install.sh --build-from-source
```

The `install.sh` script with the `--build-from-source` flag is the official way to build and install the `vm` tool for development.

## Running Tests

### Full Test Suite
```bash
# From the root of the repository
make test
```

### Staged Testing (Unit vs. Integration)

To improve performance and allow for more targeted testing, the test suite is split into two stages: unit tests and integration tests.

**Unit Tests (Fast, No Dependencies)**
```bash
make test-unit
# Equivalent to: cd rust && cargo test --workspace --lib
```
- Runs all unit tests, which are self-contained and do not require external services like Docker.
- These tests are fast and should be run frequently during development.

**Integration Tests (Slower, Requires Docker)**
```bash
make test-integration
# Equivalent to: cd rust && cargo test --workspace --test '*' --features integration
```
- Runs all integration tests, which may require Docker and interact with the filesystem.
- These tests are slower and are typically run before submitting a change.

**Skipping Integration Tests**

To run all tests *except* for the integration tests, you can use the `SKIP_INTEGRATION_TESTS` environment variable. This is useful for running a quick check of all unit tests and doc tests.

```bash
SKIP_INTEGRATION_TESTS=1 make test
```

### Individual Package Tests
```bash
# Test specific packages
cargo test --package vm-config
cargo test --package vm-provider
cargo test --package vm-installer
cargo test --package vm-temp
cargo test --package vm
cargo test --package vm-core
cargo test --package vm-cli
cargo test --package vm-package-manager
cargo test --package vm-package-server
cargo test --package vm-auth-proxy
cargo test --package vm-docker-registry
cargo test --package vm-platform
cargo test --package vm-messages

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

### Test Directory Structure (`vm/tests/`)

The integration tests for the `vm` crate are organized by feature into the following directories:

```
vm/tests/
├── cli/
│   ├── config_commands.rs      # Config CLI tests
│   └── pkg_commands.rs         # Package CLI tests
├── common/
│   └── mod.rs                  # Shared test helpers
├── networking/
│   ├── port_forwarding.rs      # Port forwarding tests
│   └── ssh_refresh.rs          # SSH worktree refresh tests
├── services/
│   └── shared_services.rs      # Multi-VM service sharing
├── vm_ops/
│   ├── create_destroy_tests.rs # Core vm start/destroy lifecycle tests
│   └── ...                     # Other vm ops tests (lifecycle, features, etc.)
└── *.rs                        # Standalone or older test files
```

### VM Operations Integration Tests (NEW)

The `vm/tests/vm_operations_integration_tests.rs` file provides comprehensive testing for all core VM commands:

**Commands Tested:**
- `create` - VM creation with and without --force flag
- `start` - Starting VMs and verifying running state
- `stop` - Stopping VMs and verifying stopped state (including force-kill)
- `restart` - Restarting VMs and state transitions
- `apply` - Re-running applying on existing VMs
- `list` - Listing all VMs
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

## Git Worktrees Feature

The Git worktree feature allows developers to work on multiple branches of the same repository simultaneously, with each worktree getting its own isolated VM environment.

### Implementation Details
- **Detection**: The `vm-config` crate detects if the current project directory is inside a Git worktree by searching for a `.git` file (for worktrees) instead of a `.git` directory (for main repositories).
- **Volume Mounting**: The provider implementation (e.g., Docker) correctly identifies the root of the main repository and the specific worktree path. It then mounts both the main repository root and the worktree-specific directory as separate volumes, ensuring all necessary files are available in the VM.
- **Automatic Remounting**: When you `vm ssh`, the system automatically detects new worktrees created after VM creation and prompts to refresh mounts. This ensures new worktrees are accessible without manual VM recreation.
- **Session Safety**: SSH session tracking prevents disruptive remounts when multiple sessions are active. Use `--force-refresh` to override (disconnects others) or `--no-refresh` to skip detection.
- **Configuration**: The feature is enabled via the `worktrees.enabled` flag in either the global `~/.vm/config.yaml` or the project-specific `vm.yaml`.

### Configuration
**Global (`~/.vm/config.yaml`):**
```yaml
worktrees:
  enabled: true
  base_path: ~/worktrees # Optional: All worktrees created here
```

**Project (`vm.yaml`):**
```yaml
worktrees:
  enabled: true # Overrides global setting
```

### Testing Strategy
- Integration tests in `vm/tests/workflow_tests.rs` cover worktree creation scenarios.
- Integration tests in `vm/tests/ssh_refresh.rs` cover automatic worktree remounting.
- Tests create a temporary Git repository, add worktrees, and verify SSH detects and offers to refresh mounts.
- Assertions verify that the VM is created successfully and that the volume mounts are correctly configured for the worktree structure.

## ServiceManager Architecture

The `ServiceManager` (`rust/vm/src/service_manager.rs`) is responsible for managing the lifecycle of shared, global services like the Docker registry, auth proxy, and shared databases.

### Key Concepts

1.  **Reference Counting**: The manager tracks how many VMs are using each service. A service is started when the first VM needs it and stopped when the last VM using it is destroyed.
2.  **State Persistence**: The state of all services (e.g., reference count, running status, port) is saved to `~/.vm/state/services.json`. This ensures that the state is preserved across CLI commands and system reboots.
3.  **Automatic Lifecycle**: `vm start` calls `register_vm_services`, and `vm destroy` calls `unregister_vm_services` to automatically manage the reference counts.

### Adding a New Service (Step-by-Step)

To add a new shared service (e.g., a new database), follow these steps:

1.  **Update `global_config.rs`**:
    *   In `rust/vm-config/src/global_config.rs`, add a new `YourServiceSettings` struct with `enabled`, `port`, `version`, etc.
    *   Add the new settings struct to the `GlobalServices` struct.
    *   Update the `is_default` method in `GlobalServices` to include your new service.
    *   Add default value functions for your service's settings (e.g., `default_your_service_port`).

2.  **Update `service_manager.rs`**:
    *   In `register_vm_services`, add a check for `global_config.services.your_service.enabled`.
    *   Add cases for `"your_service"` in the `match` statements within `start_service`, `stop_service`, `get_service_port`, and `check_service_health`.
    *   Implement `start_your_service` and `stop_your_service` methods. For Docker-based services, these methods will typically use `tokio::process::Command` to run `docker` commands.

3.  **Update `compose.rs` (for environment variables)**:
    *   In `rust/vm-provider/src/docker/compose.rs`, modify the `build_host_package_context` function.
    *   Add a new `if` block to check if your service is enabled in the global config.
    *   If it is, add the necessary environment variables (e.g., `DATABASE_URL`) to the `host_env_vars` vector.

By following this pattern, you can easily extend the `ServiceManager` to support new shared services.

## Build Commands

### Using Make (Recommended)

```bash
# Show all available targets
make help

# Build with automatic version bump (+0.0.1)
make build

# Build without version bump
make build-no-bump

# Bump version without building
make bump-version

# Run all tests
make test

# Code quality checks
make fmt             # Format code
make clippy          # Run linter
make check           # Run fmt + clippy + test
make check-duplicates # Check for code duplication
```

## Development Tools

### Code Quality Analysis

Install additional quality checking tools:

```bash
# Code duplication detection
npm install -g jscpd

# Rust code complexity analysis
cargo install rust-code-analysis-cli

# Security scanning
cargo install cargo-deny cargo-audit

# Test coverage
cargo install cargo-tarpaulin
```

### Running Quality Checks

```bash
# Check for code duplication
jscpd rust/ --threshold 2

# Check for security vulnerabilities
cd rust && cargo deny check
cd rust && cargo audit

# Check code complexity
rust-code-analysis-cli --metrics -p rust/

# Generate test coverage
cd rust && cargo tarpaulin --workspace --out Html
```

### Pre-commit Hooks

The project uses pre-commit hooks for:
- Rust formatting (`cargo fmt`)
- Clippy linting
- Quick tests for affected packages
- Commit message validation

See `.git/hooks/pre-commit` for details.

### Using Cargo Directly

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

### Version Management

The project uses automatic version bumping:

- **Version location**: `rust/Cargo.toml` workspace version field (search for `workspace.package.version`)
- **Auto-bump**: Running `make build` automatically increments the patch version (+0.0.1)
- **Manual bump**: Run `make bump-version` to bump without building
- **Version propagation**: All workspace crates inherit the version via `version.workspace = true`
- **Runtime access**: Use `env!("CARGO_PKG_VERSION")` in Rust code to access the current version

```bash
# Example version progression (actual versions will differ)
# Starting version: 2.0.6
make build          # → Bumps to 2.0.7 and builds
make bump-version   # → Bumps to 2.0.8 (no build)
```

The version bump script (`scripts/bump-version.sh`):
- Parses the current version from `rust/Cargo.toml`
- Increments the patch number (x.y.z → x.y.z+1)
- Updates `Cargo.toml` and regenerates `Cargo.lock`
- Creates backups and validates changes

## Cross-Platform Compilation

The project supports multiple target platforms for distribution.

### Supported Targets

- **Linux x86_64**: `x86_64-unknown-linux-gnu` (default)
- **Linux ARM64**: `aarch64-unknown-linux-gnu`
- **macOS Intel**: `x86_64-apple-darwin`
- **macOS Apple Silicon**: `aarch64-apple-darwin`
- **Windows**: `x86_64-pc-windows-msvc`

### Build for Specific Target

```bash
# Install target (one-time setup)
rustup target add aarch64-unknown-linux-gnu

# Build for Linux ARM64
cargo build --workspace --release --target aarch64-unknown-linux-gnu

# Build for macOS ARM64 (requires macOS host or cross-compilation tools)
cargo build --workspace --release --target aarch64-apple-darwin
```

### Cross-Compilation Setup

For Linux ARM64 on x86_64 host:
```bash
# Install cross-compilation toolchain
sudo apt-get install gcc-aarch64-linux-gnu

# Build with custom linker
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --workspace --release --target aarch64-unknown-linux-gnu
```

Cross-compilation artifacts are stored in `rust/target/<triple>/` directories and excluded from version control.

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

### Installation
```bash
# Install required tools (one-time setup)
cargo install cargo-machete cargo-audit

# cargo-audit: Security vulnerability scanning
# cargo-machete: Unused dependency detection
```

### Running Checks
```bash
source $HOME/.cargo/env
cd rust

# Check for dead code using Clippy (built-in)
cargo clippy --workspace -- -D dead_code
# Or use the alias:
cargo dead-code

# Check for unused dependencies
cargo machete
# Or use the alias:
cargo dead-deps

# Check for security vulnerabilities
cargo audit
```

### Tools
- **Clippy** (built-in): Detects unused code, imports, variables, unreachable code
- **cargo-machete**: Detects unused dependencies in Cargo.toml files
- **cargo-audit**: Scans for known security vulnerabilities in dependencies

### Configuration
- **Cargo aliases** (`.cargo/config.toml`):
  - `cargo dead-code` → `cargo clippy -- -D dead_code`
  - `cargo dead-deps` → `cargo machete`
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
