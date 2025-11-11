# Test Suite Documentation

The VM tool includes 300+ unit tests and integration tests across multiple workspace crates.

### Test Coverage by Package

| Package | Unit Tests | Integration Tests | Total |
|---------|-----------|-------------------|-------|
| vm-config | 87 | 12 | 99 |
| vm-provider | 54 | 8 | 62 |
| vm | 32 | 28 | 60 |
| vm-installer | 18 | 5 | 23 |
| Others | 61 | 0 | 61 |
| **TOTAL** | **252** | **53** | **305** |

*Updated: 2025-10-08*

## Current Test Structure

```
rust/
├── vm-config/          # Configuration operations & framework detection (includes detector/)
├── vm-temp/            # Mount validation & filesystem integration
├── vm-provider/        # Security validation & path protection
├── vm-package-manager/ # Package management operations
├── vm-installer/       # Installation management
├── vm-core/            # Common utilities and error handling
├── vm-platform/        # Platform detection and OS-specific operations
├── vm-cli/             # CLI formatting and output
└── vm/                 # CLI workflows and integration tests
```

## Test Categories

### Unit Tests (Rust modules)

**Purpose**: Test individual components and functions in isolation.

- **Framework Detection**: Tests in `vm-config/src/detector/tests/` for:
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
cargo test -p vm-config      # Configuration operations & framework detection
cargo test -p vm-temp        # Mount validation
cargo test -p vm-provider    # Security & path validation
cargo test -p vm-core        # Common utilities
cargo test -p vm-platform    # Platform detection

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

## Test Configuration

Tests use dynamic fixture generation rather than static configuration files:

- Each test creates its own temporary directories and files
- The `ProjectTestFixture` struct in `vm-config` handles test setup
- Integration tests use `CrossCrateTestFixture` for cross-crate scenarios
- No external fixture files are required

For example configurations, see the `examples/` directory at the project root.

## Migration Status

### Complete Migration to Rust

All testing functionality has been migrated from shell scripts to Rust:

| Component | Test Type | Location | Notes |
|-----------|-----------|----------|-------|
| Framework detection | Unit tests | `rust/vm-config/src/detector/tests/*.rs` | 12 test files covering all frameworks |
| Configuration | Unit & Integration | `rust/vm-config/src/` & `tests/` | Config loading, validation, presets |
| VM workflows | Integration tests | `rust/vm/tests/` | Full lifecycle testing |
| Provider security | Unit tests | `rust/vm-provider/src/` | Security validation, path protection |
| Platform detection | Unit tests | `rust/vm-platform/src/` | OS-specific operations |
| Package management | Integration tests | `rust/vm-package-manager/tests/` | Package operations |
| Core utilities | Unit tests | `rust/vm-core/src/` | Error handling, common functions |
| CLI formatting | Unit tests | `rust/vm-cli/src/` | Output formatting |
| Temp operations | Integration tests | `rust/vm-temp/tests/` | Mount validation |

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