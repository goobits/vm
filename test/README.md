# VM Test Suite

Test suite for the VM tool, focusing on configuration validation and core functionality testing.

## Structure

```
/workspace/
├── test.sh                 # Main unified test runner (root level)
└── test/
    ├── README.md           # This file
    ├── configs/            # Test configuration files (YAML format)
    │   ├── minimal.yaml
    │   ├── docker.yaml
    │   ├── services/       # Service-specific configs
    │   └── languages/      # Language package configs
    ├── docker-wrapper.sh   # Docker testing utilities
    └── test-migrate-temp.sh # Migration and temp VM tests
```

## Available Test Suites

The test runner supports these suites:
- `framework` - Basic framework functionality
- `minimal` - Minimal configuration tests  
- `services` - Service integration tests
- `languages` - Language package tests
- `cli` - Command-line interface tests
- `lifecycle` - VM lifecycle management
- `migrate-temp` - Migration and temporary VM tests

## Running Tests

### Run all tests
```bash
./test.sh
```

### Run specific test suite
```bash
./test.sh --suite minimal      # Run only minimal config tests
./test.sh --suite services     # Run only service tests  
./test.sh --suite languages    # Run only language tests
./test.sh --suite cli          # Run only CLI tests
./test.sh --suite migrate-temp # Run migration and temp VM tests
./test.sh --suite lifecycle    # Run only lifecycle tests
```

### Run with specific provider
```bash
./test.sh --provider docker   # Run with Docker only
./test.sh --provider vagrant  # Run with Vagrant only
```

### List available test suites
```bash
./test.sh --list
```

## Test Configuration

The test suite uses YAML configuration files in `/workspace/test/configs/`:

- **`minimal.yaml`** - Minimal VM configuration for basic testing
- **`docker.yaml`** - Docker-specific configuration 
- **`services/`** - Service-specific test configs (PostgreSQL, Redis, MongoDB)
- **`languages/`** - Language package test configs (npm, cargo, pip)

## Test Implementation

### Migration and Temp VM Tests (`test-migrate-temp.sh`)
Located at `/workspace/test/test-migrate-temp.sh`, this script tests:
- `vm migrate --check` functionality
- `vm migrate --dry-run` and live migration
- `vm temp` creation, status, SSH, and destroy operations
- Collision handling for existing temp VMs
- Mount validation and permissions

### Docker Wrapper (`docker-wrapper.sh`) 
Utility script for Docker-specific test operations and container management.

## Test Results

Tests use color-coded output:
- 🟢 **Green**: Passed tests
- 🔴 **Red**: Failed tests  
- 🟡 **Yellow**: Warnings or skipped tests
- 🔵 **Blue**: Test execution status

## Adding New Tests

To add new functionality testing:

1. **For migrate/temp features**: Add test functions to `test-migrate-temp.sh`
2. **For new suites**: The test runner supports adding new suite names to `AVAILABLE_SUITES`
3. **For configs**: Add new YAML configs to the appropriate subdirectory in `test/configs/`

Example test function:
```bash
test_new_feature() {
    local test_name="Testing new feature"
    echo -e "\n${BLUE}$test_name${NC}"
    
    # Test implementation here
    if command_succeeds; then
        echo -e "${GREEN}✓ $test_name passed${NC}"
        return 0
    else
        echo -e "${RED}✗ $test_name failed${NC}"
        return 1
    fi
}
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