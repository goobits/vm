# Test Suite Documentation

The VM tool test suite is implemented entirely in Rust with 165+ unit tests and integration tests across 37 files.

## Current Test Structure

```
rust/
├── vm-detector/         # Framework detection (55 tests across 12 test files)
├── vm-config/          # Configuration operations (18 tests)
├── vm-temp/            # Mount validation & filesystem integration (8 tests)
├── vm-provider/        # Security validation & path protection (22 tests)
├── vm-ports/           # Port registry and range management (13 tests)
├── vm-pkg/             # Package management operations (10 tests)
├── vm-installer/       # Installation management (11 tests)
├── vm-common/          # Common utilities (10 tests)
└── vm/                 # CLI and workflows (21 tests)
```

## Test Categories

### Unit Tests (Rust modules)

**Purpose**: Test individual components and functions in isolation.

- **Framework Detection**: Tests in `vm-detector` for:
  - React, Vue, Angular, Next.js detection
  - Python (Django, Flask) detection
  - Node.js, Rust, Go detection
  - Multi-technology project detection
  - Edge cases and error handling

- **Configuration Operations**: Tests in `vm-config` for:
  - YAML configuration validation
  - Configuration file parsing and merging
  - Error handling for malformed configs
  - Preset detection and application

### Integration Tests (Rust `tests/` directories)

**Purpose**: Test how components work together and integrate with external systems.

- **Configuration System**: Tests configuration loading, merging, and validation
- **Preset System**: Tests preset detection, application, and command functionality
- **Mount Operations**: Tests filesystem integration and security validation
- **Port Management**: Tests port allocation and conflict detection

### System Tests (End-to-end workflows)

**Currently implemented via Rust integration tests** that cover:
- VM lifecycle operations (creation, destruction, status)
- Command execution and shell integration
- Provider-specific workflows (Docker, Vagrant, Tart)
- Configuration validation across complete workflows

## Running All Tests

### Rust Test Framework

All testing is now done via Rust's built-in test framework:

```bash
# Run all tests
cd rust && cargo test --workspace

# Run tests with output
cargo test --workspace -- --nocapture

# Run specific component tests
cargo test -p vm-detector    # Framework detection
cargo test -p vm-config      # Configuration operations
cargo test -p vm-temp        # Mount validation
cargo test -p vm-provider    # Security & path validation
cargo test -p vm-ports       # Port management

# Run integration tests only
cargo test --test integration_tests
```

### Test Development

**Adding New Tests**:

Create tests in the appropriate Rust module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_your_feature() {
        // Test implementation
    }
}
```

**Test Organization**:
- Unit tests: In the same file as the code (`#[cfg(test)]` modules)
- Integration tests: In `tests/` directory of each crate
- Use `tempfile` crate for filesystem testing (workspace dependency)

**Example Test Patterns**:

```rust
// Unit test
#[test]
fn test_port_range_parsing() {
    let range = PortRange::parse("3000-3009").unwrap();
    assert_eq!(range.start(), 3000);
    assert_eq!(range.end(), 3009);
}

// Integration test
#[test]
fn test_config_loading_workflow() {
    let config = VmConfig::load(None, false)?;
    assert!(config.provider.is_some());
}
## Test Dependencies

### Prerequisites

- **Rust toolchain**: Required for all testing (`cargo test`)
- **Docker**: Required for provider integration tests
- **Git**: Required for project detection tests

### Docker Permissions

Integration tests that use Docker require proper permissions:

```bash
# Add user to docker group
sudo usermod -aG docker $USER

# Restart session
newgrp docker

# Or ensure Docker daemon is running
sudo systemctl start docker
```

## Test Configuration Files

Test fixtures are located in `../fixtures/configs/`:

- Service configs: `postgresql.yaml`, `redis.yaml`, `mongodb.yaml`, `docker.yaml`
- Minimal configuration: `minimal.yaml`
- Language packages: `languages/npm_packages.yaml`, `languages/pip_packages.yaml`, `languages/cargo_packages.yaml`

These YAML files provide test data for configuration validation and integration tests.

## Migration Status

### Complete Migration to Rust

All testing functionality has been migrated from shell scripts to Rust:

| Component | Test Type | Location | Test Count |
|-----------|-----------|----------|------------|
| Framework detection | Unit tests | `rust/vm-detector/src/tests/*.rs` | 55 tests |
| Configuration | Unit & Integration | `rust/vm-config/src/` & `tests/` | 18 tests |
| VM workflows | Integration tests | `rust/vm/tests/` | 21 tests |
| Provider security | Unit tests | `rust/vm-provider/src/` | 22 tests |
| Port management | Unit tests | `rust/vm-ports/src/` | 13 tests |
| Package management | Integration tests | `rust/vm-pkg/tests/` | 10 tests |
| Installer | Unit tests | `rust/vm-installer/src/` | 11 tests |
| Temp operations | Integration tests | `rust/vm-temp/tests/` | 8 tests |

### Benefits of Rust Migration

- **Type Safety**: Compile-time error detection
- **Performance**: Faster test execution
- **Reliability**: Better error handling and resource management
- **Maintainability**: Integrated with code, automatic dependency management

## Test Development Guidelines

### Adding New Tests

1. **Unit Tests**: Add `#[test]` functions to the relevant module
2. **Integration Tests**: Create files in the `tests/` directory of relevant crates
3. **End-to-end Tests**: Use integration tests that exercise complete workflows

### Test Naming Convention

- Use descriptive test function names: `test_framework_detection()`, `test_config_loading()`
- Use clear assertion messages with context
- Group related tests in logical modules (`#[cfg(test)] mod tests`)

### Best Practices

- Keep unit tests fast and isolated
- Use `tempfile` for filesystem tests to avoid conflicts
- Include both positive and negative test cases
- Provide clear error messages and debugging output
- Use appropriate test fixtures and mocking where needed

## Troubleshooting

### Common Issues

1. **Docker Permission Denied**: See Docker Permissions section above
2. **Test Failures**: Run with `--nocapture` for detailed output
3. **Compilation Errors**: Ensure Rust toolchain is up to date
4. **Container Conflicts**: Integration tests clean up automatically, but manual cleanup may be needed:
   ```bash
   docker ps -a | grep "test-" | awk '{print $1}' | xargs docker rm -f
   ```

### Debug Mode

Enable verbose test output:
```bash
# Show test output
cargo test -- --nocapture

# Show test names being run
cargo test -- --nocapture --show-output

# Run specific test with debugging
RUST_LOG=debug cargo test test_name -- --nocapture
```

### Getting Help

Use Rust's built-in test help:
```bash
# Show available test options
cargo test --help

# List all available tests
cargo test -- --list

# Run tests matching a pattern
cargo test config -- --nocapture
```