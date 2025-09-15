# VM Tool Test Suite

## Overview

The VM tool test suite has been migrated from shell scripts to Rust. The previous shell test suite (2,773 lines) was removed because:
- Missing dependencies (docker-utils.sh was removed in commit 7640f9e)
- Broken function references (check_docker_access, detect_project_type never existed)
- Superseded by comprehensive Rust tests

## Current Test Coverage

### Rust Tests (Working)
- **vm-detector**: 22 tests for project type detection
- **vm-config**: Configuration operations, validation, merging
- **vm-temp**: Mount validation, edge cases, filesystem integration
- **vm-provider**: Security validation, path traversal protection
- **Resource suggestions**: VM resource recommendations by project type

### Running Tests

```bash
# Run all Rust tests
cd rust
cargo test --workspace

# Run specific component tests
cargo test -p vm-detector
cargo test -p vm-config
cargo test -p vm-temp

# Run with output
cargo test -- --nocapture

# Quick test script from project root
./test-rust.sh
```

## Test Artifacts

The `test/configs/` directory contains YAML configuration files used for testing:
- `minimal.yaml` - Basic VM configuration
- `postgresql.yaml`, `redis.yaml`, `mongodb.yaml` - Service configurations
- `languages/` - Language-specific package configurations
- `test-json-reject/` - JSON rejection test files

## Migration Notes

### What Was Removed
- `/workspace/run-tests.sh` (1,271 lines) - Main test runner
- `/workspace/test/integration/preset-system.test.sh` (716 lines)
- `/workspace/test/system/vm-lifecycle.test.sh` (564 lines)
- `/workspace/setup-tests.sh` (222 lines)

### Why Removed
1. All shell tests depended on `shared/docker-utils.sh` which was removed
2. Functions like `detect_project_type()` were never implemented
3. Rust tests provide better coverage with type safety
4. Infrastructure testing belongs in proper tools (Terraform, Ansible)

## Future Testing Strategy

### Integration Testing
For VM lifecycle and service provisioning tests, consider:
- **testcontainers** crate for Docker integration tests
- **Terraform** for infrastructure validation
- **Ansible** for provisioning verification

### What's Not Currently Tested
- Actual VM creation/destruction (requires Docker)
- Service installation verification (PostgreSQL, Redis, etc.)
- Cross-process command execution

These are infrastructure concerns better tested with infrastructure tools.

## Development

### Adding New Tests

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

### Test Organization
- Unit tests: In the same file as the code
- Integration tests: In `tests/` directory of each crate
- Use `tempfile` crate for filesystem testing
- Use `mockall` for mocking external dependencies

## Quick Reference

```bash
# Build everything
cargo build --release

# Test everything
cargo test --workspace

# Test with coverage (requires cargo-tarpaulin)
cargo tarpaulin --workspace

# Run benchmarks (if any)
cargo bench

# Check for issues
cargo clippy -- -D warnings
```