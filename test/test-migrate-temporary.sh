#!/bin/bash
# Test suite for vm migrate and vm temp commands
# This script tests the new Phase 2 and Phase 3 features

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get script directory (removed unused variable)

# Test results
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
FAILED_TEST_NAMES=()

# Helper functions
run_test() {
    local test_name="$1"
    local test_function="$2"

    echo -e "\n${BLUE}Running test: $test_name${NC}"

    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    # Run test in a subshell to isolate failures
    if (
        set -e
        $test_function
    ); then
        echo -e "${GREEN}✓ Test passed: $test_name${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
        return 0
    else
        echo -e "${RED}✗ Test failed: $test_name${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        FAILED_TEST_NAMES+=("$test_name")
        return 1
    fi
}

assert_file_exists() {
    local file="$1"
    local description="${2:-File should exist: $file}"

    if [ -f "$file" ]; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        return 1
    fi
}

assert_file_not_exists() {
    local file="$1"
    local description="${2:-File should not exist: $file}"

    if [ ! -f "$file" ]; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        return 1
    fi
}

assert_command_succeeds() {
    local command="$1"
    local description="${2:-Command should succeed}"

    if $command > /dev/null 2>&1; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        return 1
    fi
}

assert_command_fails() {
    local command="$1"
    local description="${2:-Command should fail}"

    if ! $command > /dev/null 2>&1; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description (command succeeded unexpectedly)${NC}"
        return 1
    fi
}

assert_output_contains() {
    local command="$1"
    local expected="$2"
    local description="${3:-Output should contain: $expected}"

    local output
    output=$($command 2>&1)
    if echo "$output" | grep -q "$expected"; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        echo "  Output: $output"
        return 1
    fi
}

# ============================================================================
# Test Suite: vm migrate Tests
# ============================================================================

# Test vm migrate --check with no files
test_migrate_check_no_files() {
    echo "Testing vm migrate --check with no files..."

    # Create test directory
    local test_dir="/tmp/migrate-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"

    # Test when neither file exists
    assert_output_contains "vm migrate --check" "No migration needed" \
        "Should report no migration needed when no files exist"

    # Cleanup
    cd - > /dev/null
    rm -rf "$test_dir"
}

# Test vm migrate --check with vm.json
test_migrate_check_with_json() {
    echo "Testing vm migrate --check with vm.json..."

    # Create test directory
    local test_dir="/tmp/migrate-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"

    # Create sample vm.json
    cat > vm.json << 'EOF'
{
  "project": {
    "name": "test-project",
    "workspace_path": "/workspace"
  },
  "provider": "docker",
  "services": {
    "postgresql": {
      "enabled": true
    }
  }
}
EOF

    # Test check mode
    assert_output_contains "vm migrate --check" "Migration recommended" \
        "Should recommend migration when vm.json exists"

    # Cleanup
    cd - > /dev/null
    rm -rf "$test_dir"
}

# Test vm migrate --check with both files
test_migrate_check_with_both() {
    echo "Testing vm migrate --check with both files..."

    # Create test directory
    local test_dir="/tmp/migrate-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"

    # Create both files
    echo '{"project": {"name": "test"}}' > vm.json
    echo 'version: "1.0"' > vm.yaml

    # Test check mode
    assert_output_contains "vm migrate --check" "Both vm.json and vm.yaml exist" \
        "Should detect when both files exist"

    # Cleanup
    cd - > /dev/null
    rm -rf "$test_dir"
}

# Test vm migrate --dry-run
test_migrate_dry_run() {
    echo "Testing vm migrate --dry-run..."

    # Create test directory
    local test_dir="/tmp/migrate-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"

    # Create sample vm.json
    cat > vm.json << 'EOF'
{
  "project": {
    "name": "test-project",
    "workspace_path": "/workspace"
  },
  "provider": "docker",
  "services": {
    "postgresql": {
      "enabled": true
    }
  }
}
EOF

    # Test dry run - capture output
    local output
    output=$(vm migrate --dry-run 2>&1)

    # Check output contains YAML
    if echo "$output" | grep -q "version: \"1.0\""; then
        echo -e "${GREEN}✓ Dry run output contains version field${NC}"
    else
        echo -e "${RED}✗ Dry run output should contain version field${NC}"
        echo "Output: $output"
        return 1
    fi

    # Check output contains original data
    if echo "$output" | grep -q "test-project"; then
        echo -e "${GREEN}✓ Dry run output contains project name${NC}"
    else
        echo -e "${RED}✗ Dry run output should contain project name${NC}"
        return 1
    fi

    # Ensure no files were created
    assert_file_not_exists "vm.yaml" "Dry run should not create vm.yaml"
    assert_file_exists "vm.json" "Dry run should not modify vm.json"

    # Cleanup
    cd - > /dev/null
    rm -rf "$test_dir"
}

# Test live migration
test_migrate_live() {
    echo "Testing live migration..."

    # Create test directory
    local test_dir="/tmp/migrate-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"

    # Create sample vm.json
    cat > vm.json << 'EOF'
{
  "project": {
    "name": "test-project",
    "hostname": "test.local",
    "workspace_path": "/workspace"
  },
  "provider": "docker",
  "terminal": {
    "emoji": "🧪",
    "username": "developer"
  },
  "services": {
    "postgresql": {
      "enabled": true,
      "port": 5432
    },
    "redis": {
      "enabled": false
    }
  },
  "aliases": {
    "gs": "git status",
    "ll": "ls -la"
  }
}
EOF

    # Run migration (force to skip prompt)
    vm migrate --force

    # Check files were created
    assert_file_exists "vm.yaml" "Migration should create vm.yaml"
    assert_file_exists "vm.json.bak" "Migration should create backup"
    assert_file_not_exists "vm.json" "Migration should remove original (with --force)"

    # Check version field exists
    local version
    version=$(yq -r '.version' vm.yaml)
    if [ "$version" = "1.0" ]; then
        echo -e "${GREEN}✓ Version field is present and correct${NC}"
    else
        echo -e "${RED}✗ Version field should be '1.0', got: $version${NC}"
        return 1
    fi

    # Check content was preserved
    local project_name
    project_name=$(yq -r '.project.name' vm.yaml)
    if [ "$project_name" = "test-project" ]; then
        echo -e "${GREEN}✓ Project name was preserved${NC}"
    else
        echo -e "${RED}✗ Project name should be preserved${NC}"
        return 1
    fi

    # Check services
    local pg_enabled
    pg_enabled=$(yq -r '.services.postgresql.enabled' vm.yaml)
    if [ "$pg_enabled" = "true" ]; then
        echo -e "${GREEN}✓ Service configuration was preserved${NC}"
    else
        echo -e "${RED}✗ Service configuration should be preserved${NC}"
        return 1
    fi

    # Validate the migrated config
    assert_command_succeeds "vm validate" "Migrated config should be valid"

    # Cleanup
    cd - > /dev/null
    rm -rf "$test_dir"
}

# Test JSON config rejection
test_json_config_rejection() {
    echo "Testing JSON config rejection..."

    # Create test directory
    local test_dir="/tmp/json-reject-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"

    # Create a JSON config file
    cat > config.json << 'EOF'
{
  "project": {
    "name": "test-project"
  },
  "provider": "docker"
}
EOF

    # Try to use JSON config - should fail
    if vm --config config.json status 2>&1 | grep -q "JSON configs are no longer supported"; then
        echo -e "${GREEN}✓ JSON config was properly rejected with helpful message${NC}"
    else
        echo -e "${RED}✗ JSON config should be rejected with migration message${NC}"
        cd - > /dev/null
        rm -rf "$test_dir"
        return 1
    fi

    # Verify migration suggestion is shown
    if vm --config config.json status 2>&1 | grep -q "vm migrate --input"; then
        echo -e "${GREEN}✓ Migration command suggestion shown${NC}"
    else
        echo -e "${RED}✗ Should show migration command suggestion${NC}"
        cd - > /dev/null
        rm -rf "$test_dir"
        return 1
    fi

    # Cleanup
    cd - > /dev/null
    rm -rf "$test_dir"
}

# ============================================================================
# Test Suite: vm temp Tests
# ============================================================================

# Test vm temp creation
test_temp_creation() {
    echo "Testing vm temp creation..."

    # Clean up any existing temp VM state
    rm -f ~/.vm/temp-vm.state

    # Create test directory to mount
    local test_dir="/tmp/temp-test-$$"
    mkdir -p "$test_dir/src"
    echo "test file" > "$test_dir/src/test.txt"

    cd "$test_dir"

    # Create temp VM
    vm temp ./src
    local exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}✓ Temp VM created successfully${NC}"
    else
        echo -e "${RED}✗ Temp VM creation failed${NC}"
        rm -rf "$test_dir"
        return 1
    fi

    # Check state file exists
    assert_file_exists "$HOME/.vm/temp-vm.state" "State file should be created"

    # Check state file contains correct info
    if [ -f "$HOME/.vm/temp-vm.state" ]; then
        local container_name
        container_name=$(yq -r '.container_name' "$HOME/.vm/temp-vm.state")
        if [[ "$container_name" =~ ^temp-vm- ]]; then
            echo -e "${GREEN}✓ State file contains valid container name${NC}"
        else
            echo -e "${RED}✗ State file should contain valid container name${NC}"
            return 1
        fi
    fi

    # Cleanup will be done after all temp tests
    TEMP_TEST_DIR="$test_dir"
}

# Test vm temp status
test_temp_status() {
    echo "Testing vm temp status..."

    # Requires temp VM from previous test
    if [ ! -f "$HOME/.vm/temp-vm.state" ]; then
        echo -e "${YELLOW}⚠ Skipping - no temp VM exists${NC}"
        return 0
    fi

    # Test status command
    local output
    output=$(vm temp status 2>&1)

    # Check output contains expected information
    if echo "$output" | grep -q "Running"; then
        echo -e "${GREEN}✓ Status shows VM is running${NC}"
    else
        echo -e "${RED}✗ Status should show VM is running${NC}"
        echo "Output: $output"
        return 1
    fi

    if echo "$output" | grep -q "temp-vm-"; then
        echo -e "${GREEN}✓ Status shows container name${NC}"
    else
        echo -e "${RED}✗ Status should show container name${NC}"
        return 1
    fi
}

# Test vm temp ssh
test_temp_ssh() {
    echo "Testing vm temp ssh..."

    # Requires temp VM from previous test
    if [ ! -f "$HOME/.vm/temp-vm.state" ]; then
        echo -e "${YELLOW}⚠ Skipping - no temp VM exists${NC}"
        return 0
    fi

    # Test SSH with command
    local output
    output=$(vm temp ssh -c "echo hello from temp vm" 2>&1)

    if echo "$output" | grep -q "hello from temp vm"; then
        echo -e "${GREEN}✓ SSH command execution works${NC}"
    else
        echo -e "${RED}✗ SSH command should execute${NC}"
        echo "Output: $output"
        return 1
    fi

    # Test file is accessible
    output=$(vm temp ssh -c "cat /workspace/src/test.txt" 2>&1)

    if echo "$output" | grep -q "test file"; then
        echo -e "${GREEN}✓ Mounted files are accessible${NC}"
    else
        echo -e "${RED}✗ Mounted files should be accessible${NC}"
        return 1
    fi
}

# Test vm temp collision (same mounts)
test_temp_collision_same() {
    echo "Testing vm temp collision with same mounts..."

    # Requires temp VM from previous test
    if [ ! -f "$HOME/.vm/temp-vm.state" ] || [ -z "$TEMP_TEST_DIR" ]; then
        echo -e "${YELLOW}⚠ Skipping - no temp VM exists${NC}"
        return 0
    fi

    cd "$TEMP_TEST_DIR"

    # Try to create another temp VM with same mount
    local output
    output=$(vm temp ./src 2>&1)

    if echo "$output" | grep -q "Connecting to existing"; then
        echo -e "${GREEN}✓ Detects and reuses existing VM with same mounts${NC}"
    else
        echo -e "${RED}✗ Should detect and reuse existing VM${NC}"
        echo "Output: $output"
        return 1
    fi
}

# Test vm temp destroy
test_temp_destroy() {
    echo "Testing vm temp destroy..."

    # Requires temp VM from previous test
    if [ ! -f "$HOME/.vm/temp-vm.state" ]; then
        echo -e "${YELLOW}⚠ Skipping - no temp VM exists${NC}"
        return 0
    fi

    # Get container name before destroy
    local container_name
    container_name=$(yq -r '.container_name' "$HOME/.vm/temp-vm.state")

    # Destroy temp VM
    vm temp destroy

    # Check state file is removed
    assert_file_not_exists "$HOME/.vm/temp-vm.state" "State file should be removed"

    # Check container is removed
    if docker ps -a --format '{{.Names}}' | grep -q "^${container_name}$"; then
        echo -e "${RED}✗ Container should be removed${NC}"
        return 1
    else
        echo -e "${GREEN}✓ Container was removed${NC}"
    fi

    # Cleanup test directory
    if [ -n "$TEMP_TEST_DIR" ]; then
        rm -rf "$TEMP_TEST_DIR"
    fi
}

# Test vm tmp alias
test_tmp_alias() {
    echo "Testing vm tmp alias..."

    # Create test directory
    local test_dir="/tmp/tmp-alias-test-$$"
    mkdir -p "$test_dir/data"
    echo "alias test" > "$test_dir/data/test.txt"

    cd "$test_dir"

    # Test tmp alias
    vm tmp ./data

    # Check it created temp VM
    assert_file_exists "$HOME/.vm/temp-vm.state" "tmp alias should create temp VM"

    # Cleanup
    vm temp destroy
    rm -rf "$test_dir"
}

# ============================================================================
# Main Test Runner
# ============================================================================

generate_test_report() {
    local passed=$1
    local failed=$2
    local total=$((passed + failed))

    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}Test Summary${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "Total tests: $total"
    echo -e "${GREEN}Passed: $passed${NC}"
    echo -e "${RED}Failed: $failed${NC}"

    if [ "$failed" -eq 0 ]; then
        echo -e "\n${GREEN}✓ All tests passed!${NC}"
        return 0
    else
        echo -e "\n${RED}✗ Some tests failed${NC}"
        echo -e "\n${RED}Failed tests:${NC}"
        for test_name in "${FAILED_TEST_NAMES[@]}"; do
            echo -e "  ${RED}✗ $test_name${NC}"
        done
        return 1
    fi
}

main() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}VM Migrate and Temp Commands Test Suite${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    # Make vm command available
    export PATH="/workspace:$PATH"

    # Clean up any existing temp VMs before starting
    if [ -f "$HOME/.vm/temp-vm.state" ]; then
        echo "Cleaning up existing temp VM..."
        vm temp destroy 2>/dev/null || true
    fi

    # Run vm migrate tests
    echo -e "\n${BLUE}Running vm migrate tests...${NC}"
    run_test "migrate-check-no-files" test_migrate_check_no_files
    run_test "migrate-check-with-json" test_migrate_check_with_json
    run_test "migrate-check-with-both" test_migrate_check_with_both
    run_test "migrate-dry-run" test_migrate_dry_run
    run_test "migrate-live" test_migrate_live

    # Run JSON deprecation test
    echo -e "\n${BLUE}Running JSON deprecation test...${NC}"
    run_test "json-config-rejection" test_json_config_rejection

    # Run vm temp tests
    echo -e "\n${BLUE}Running vm temp tests...${NC}"
    run_test "temp-creation" test_temp_creation
    run_test "temp-status" test_temp_status
    run_test "temp-ssh" test_temp_ssh
    run_test "temp-collision-same" test_temp_collision_same
    run_test "temp-destroy" test_temp_destroy
    run_test "tmp-alias" test_tmp_alias

    # Generate final report
    generate_test_report $PASSED_TESTS $FAILED_TESTS

    # Exit with appropriate code
    [ $FAILED_TESTS -eq 0 ]
}

# Run if executed directly
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi