# VM Test Suite

Comprehensive test suite for the VM tool, ensuring both Docker and Vagrant providers work correctly across various configurations.

## Structure

```
test/
├── test-suite.sh           # Main test runner
├── lib/
│   ├── test-helpers.sh     # Common test functions
│   └── config-generator.sh # Generate test configurations
├── configs/                # Generated test configurations
└── suites/                 # Test suites
    ├── 01-minimal.sh       # Minimal configuration tests
    ├── 02-services.sh      # Service toggle tests
    ├── 03-languages.sh     # Language package tests
    ├── 04-cli.sh           # CLI command tests
    └── 05-lifecycle.sh     # VM lifecycle tests
```

## Running Tests

### Run all tests
```bash
./test/test-suite.sh
```

### Run specific test suite
```bash
./test/test-suite.sh minimal      # Run only minimal config tests
./test/test-suite.sh services     # Run only service tests
./test/test-suite.sh languages    # Run only language tests
./test/test-suite.sh cli          # Run only CLI tests
./test/test-suite.sh lifecycle    # Run only lifecycle tests
```

### Run with specific provider
```bash
./test/test-suite.sh all docker   # Run all tests with Docker only
./test/test-suite.sh all vagrant  # Run all tests with Vagrant only
./test/test-suite.sh all both     # Run all tests with both providers
```

## Test Suites

### 1. Minimal Configuration Tests (`01-minimal.sh`)
- Tests VM works with absolute minimum configuration
- Verifies no unnecessary services are installed
- Ensures basic functionality (shell, workspace mounting)
- Tests extensibility of minimal configs

### 2. Service Toggle Tests (`02-services.sh`)
- Tests each service (PostgreSQL, Redis, MongoDB, Docker) in isolation
- Verifies services can be enabled/disabled independently
- Tests multiple services together
- Verifies custom port configurations
- Tests service persistence across VM restarts

### 3. Language Package Tests (`03-languages.sh`)
- Tests npm package installation
- Tests cargo packages trigger Rust installation
- Tests pip packages trigger Python/pyenv installation
- Verifies empty package arrays don't install runtimes
- Tests multiple languages together

### 4. CLI Command Tests (`04-cli.sh`)
- Tests `vm init` creates customized configurations
- Tests `vm validate` detects valid/invalid configs
- Tests `vm status` reports correct VM state
- Tests `vm list` shows running VMs
- Tests `vm exec` runs commands correctly
- Tests `vm reload` applies configuration changes
- Tests `vm destroy` removes VMs cleanly

### 5. Lifecycle Tests (`05-lifecycle.sh`)
- Tests halt/resume preserves VM state
- Tests configuration reload applies changes
- Tests adding services via reload
- Tests workspace synchronization
- Tests rapid lifecycle transitions
- Tests destroy and recreate workflows

## Test Helpers

The test suite includes helper functions for common assertions:

- `assert_vm_running` - Verify VM is running
- `assert_vm_stopped` - Verify VM is stopped
- `assert_command_succeeds` - Verify command exits with 0
- `assert_command_fails` - Verify command exits with non-0
- `assert_file_exists` - Verify file exists in VM
- `assert_output_contains` - Verify command output contains string
- `assert_service_enabled` - Verify service is installed and available
- `assert_service_not_enabled` - Verify service is not installed

## Adding New Tests

1. Create a new test suite file in `suites/` (e.g., `06-newfeature.sh`)
2. Source the test helpers at the top
3. Write test functions prefixed with `test_`
4. Use assertion helpers for consistency
5. Make the file executable: `chmod +x suites/06-newfeature.sh`

Example test function:
```bash
test_my_feature() {
    echo "Testing my new feature..."
    
    create_test_vm "$CONFIG_DIR/my-config.json" || return 1
    
    assert_command_succeeds "my-command" "Command should work"
    assert_output_contains "my-command" "expected output" "Output check"
}
```

## Configuration Generator

The test suite can generate configurations programmatically:

```bash
# Generate a custom config
generate_config "my-test" '{"project":{"name":"custom"}}' "/tmp/custom.json"

# Generate service-specific config
generate_service_config "postgresql" true

# Generate package config
generate_package_config "npm_packages" '["prettier", "eslint"]'
```

## Prerequisites

- Docker and/or Vagrant installed
- jq for JSON manipulation
- timeout command (part of coreutils)
- Basic Unix tools (grep, sed, awk)

## Troubleshooting

- Tests create VMs in `/tmp/vm-test-*` directories
- Each test cleans up after itself via trap handlers
- If cleanup fails, manually remove test directories and destroy test VMs
- Use `vm list` to see any leftover test VMs
- Check test output for specific failure reasons