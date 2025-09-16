# Claude Development Notes

## Running Tests

To run the Rust test suite:
```bash
source $HOME/.cargo/env
cd rust
cargo test --workspace
```

## Running Individual Test Modules

```bash
# Run specific test file
cargo test --package vm-config config_ops_tests

# Run integration tests
cargo test integration_tests

# Run with output
cargo test -- --nocapture
```

## Build Commands

```bash
# Build all packages
cargo build --workspace

# Build in release mode
cargo build --workspace --release

# Check without building
cargo check --workspace
```