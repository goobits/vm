# Test Suite Setup Guide

## Quick Start

```bash
# Setup test environment (one-time)
./setup-tests.sh

# Run all tests
./run-tests.sh

# Run specific test suite
./run-tests.sh --suite framework
./run-tests.sh --suite minimal
./run-tests.sh --suite services
```

## Prerequisites

### 1. Docker (Required for most tests)
- Install Docker: `sudo apt-get install docker.io` (Ubuntu/Debian)
- Check Docker status: `docker --version`

### 2. Docker Permissions (Recommended)
Many tests require Docker access without sudo. To enable:

```bash
# Add your user to the docker group
sudo usermod -aG docker $USER

# Apply changes (logout/login or run):
newgrp docker

# Verify access
docker ps
```

**Note:** Tests will skip Docker-dependent functionality if permissions aren't configured, showing helpful warnings instead of failing.

### 3. Rust Toolchain (Required for vm-config)
The vm-config tool is written in Rust. To build it:

```bash
# Install Rust if not present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build vm-config
cd rust/vm-config
cargo build --release
```

Alternatively, run `./install.sh` which handles Rust installation and building automatically.

## Running Tests

### Available Test Suites

| Suite | Description | Docker Required |
|-------|-------------|-----------------|
| `framework` | Configuration validation, vm command tests | No |
| `minimal` | Basic VM functionality | Yes |
| `services` | PostgreSQL, Redis, MongoDB, Docker integration | Yes |
| `languages` | Node.js, Python support | Yes |
| `cli` | CLI commands (init, validate, status, exec) | Partial |
| `lifecycle` | VM creation, destruction, reload | Yes |

### Individual Test Files

```bash
# Unit tests
./test/unit/config-validation.test.sh
./test/unit/preset-detection.test.sh

# Integration tests
./test/integration/preset-system.test.sh

# System tests
./test/system/vm-lifecycle.test.sh
```

### Verbose Mode

```bash
# Enable detailed output
VERBOSE=true ./run-tests.sh --suite minimal
```

## Common Issues & Solutions

### Issue: "Docker access unavailable" warnings
**Solution:** Add user to docker group (see Prerequisites section)

### Issue: "vm init command not found"
**Solution:** The log_info function issue has been fixed. Pull latest changes.

### Issue: "vm-config: command not found"
**Solution:** Run `./install.sh` or build manually:
```bash
cd rust/vm-config && cargo build --release
```

### Issue: Tests hanging or timing out
**Solution:** Check Docker daemon is running:
```bash
sudo systemctl status docker
sudo systemctl start docker  # If not running
```

### Issue: Permission denied errors
**Solution:** Ensure proper file permissions:
```bash
chmod +x *.sh
chmod +x test/**/*.sh
```

## Test Output

Tests use colored output:
- 🟢 **Green** - Test passed
- 🔴 **Red** - Test failed
- 🟡 **Yellow** - Test skipped (usually due to missing permissions)

## Development Notes

### Writing New Tests

1. Add test functions to appropriate category:
   - `test/unit/` - Isolated component tests
   - `test/integration/` - Component interaction tests
   - `test/system/` - End-to-end workflow tests

2. Use provided assertion helpers:
   - `assert_command_succeeds`
   - `assert_file_exists`
   - `assert_service_enabled`
   - `assert_vm_running`

3. Follow naming conventions:
   - Test functions: `test_feature_name()`
   - Test files: `feature-name.test.sh`

### Debugging Tests

```bash
# Run with debug output
set -x
./test/unit/config-validation.test.sh

# Check test artifacts
ls -la .test_artifacts/

# Clean up test artifacts
rm -rf .test_artifacts/
```

## Getting Help

```bash
# Show test runner help
./run-tests.sh --help

# List available test suites
./run-tests.sh --list
```