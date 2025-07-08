#!/bin/bash
# Test Helper Library - Common functions for all test suites

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test state
TEST_DIR=""
TEST_NAME=""
TEST_PROVIDER=""
CLEANUP_COMMANDS=()

# Initialize test environment
setup_test_env() {
    local test_name="$1"
    local provider="${2:-docker}"
    
    TEST_NAME="$test_name"
    TEST_PROVIDER="$provider"
    TEST_DIR="/tmp/vm-test-${test_name}-$$"
    
    mkdir -p "$TEST_DIR"
    cd "$TEST_DIR"
    
    # Register cleanup
    trap cleanup_test_env EXIT
    
    echo -e "${BLUE}Setting up test: $test_name (provider: $provider)${NC}"
}

# Cleanup test environment
cleanup_test_env() {
    echo -e "${BLUE}Cleaning up test environment...${NC}"
    
    # Run any registered cleanup commands
    for cmd in "${CLEANUP_COMMANDS[@]}"; do
        eval "$cmd" 2>/dev/null || true
    done
    
    # Destroy VM if it exists
    if [ -f "$TEST_DIR/vm.json" ]; then
        cd "$TEST_DIR"
        vm destroy -f 2>/dev/null || true
    fi
    
    # Remove test directory
    rm -rf "$TEST_DIR"
}

# Register a cleanup command
register_cleanup() {
    CLEANUP_COMMANDS+=("$1")
}

# Create a test VM with given config
create_test_vm() {
    local config_path="$1"
    local timeout="${2:-300}"  # 5 minute default timeout
    
    echo -e "${BLUE}Creating test VM with config: $config_path${NC}"
    
    # Copy config to test directory
    if [ -f "$config_path" ]; then
        cp "$config_path" "$TEST_DIR/vm.json"
    else
        echo -e "${RED}Config file not found: $config_path${NC}"
        return 1
    fi
    
    # Start VM with timeout
    cd "$TEST_DIR"
    timeout "$timeout" vm up || {
        echo -e "${RED}Failed to create VM within ${timeout}s${NC}"
        return 1
    }
    
    # Give VM a moment to stabilize
    sleep 5
    
    # Verify VM is running
    assert_vm_running
}

# Run command in VM
run_in_vm() {
    local command="$1"
    local expected_exit="${2:-0}"
    
    cd "$TEST_DIR"
    # Use sudo for docker commands if needed
    if [[ "$TEST_PROVIDER" == "docker" ]] && ! docker version &>/dev/null; then
        sudo -E vm exec "$command"
    else
        vm exec "$command"
    fi
    local exit_code=$?
    
    if [ "$expected_exit" != "any" ] && [ $exit_code -ne $expected_exit ]; then
        echo -e "${RED}Command failed with exit code $exit_code (expected $expected_exit): $command${NC}"
        return 1
    fi
    
    return $exit_code
}

# Get output from VM command
get_vm_output() {
    local command="$1"
    cd "$TEST_DIR"
    vm exec "$command" 2>/dev/null
}

# Check if VM is running
is_vm_running() {
    cd "$TEST_DIR"
    vm status 2>/dev/null | grep -q "running"
}

# Assert VM is running
assert_vm_running() {
    if is_vm_running; then
        echo -e "${GREEN}✓ VM is running${NC}"
        return 0
    else
        echo -e "${RED}✗ VM is not running${NC}"
        return 1
    fi
}

# Assert VM is stopped
assert_vm_stopped() {
    if ! is_vm_running; then
        echo -e "${GREEN}✓ VM is stopped${NC}"
        return 0
    else
        echo -e "${RED}✗ VM is still running${NC}"
        return 1
    fi
}

# Assert command succeeds
assert_command_succeeds() {
    local command="$1"
    local description="${2:-Command should succeed}"
    
    if run_in_vm "$command" 0; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        return 1
    fi
}

# Assert command fails
assert_command_fails() {
    local command="$1"
    local description="${2:-Command should fail}"
    
    if run_in_vm "$command" any; then
        if [ $? -ne 0 ]; then
            echo -e "${GREEN}✓ $description${NC}"
            return 0
        fi
    fi
    
    echo -e "${RED}✗ $description (command succeeded unexpectedly)${NC}"
    return 1
}

# Assert file exists in VM
assert_file_exists() {
    local file="$1"
    local description="${2:-File should exist: $file}"
    
    if run_in_vm "test -f $file" 0; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        return 1
    fi
}

# Assert file does not exist in VM
assert_file_not_exists() {
    local file="$1"
    local description="${2:-File should not exist: $file}"
    
    if run_in_vm "test -f $file" any && [ $? -ne 0 ]; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        return 1
    fi
}

# Assert output contains string
assert_output_contains() {
    local command="$1"
    local expected="$2"
    local description="${3:-Output should contain: $expected}"
    
    local output=$(get_vm_output "$command")
    if echo "$output" | grep -q "$expected"; then
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    else
        echo -e "${RED}✗ $description${NC}"
        echo "  Output: $output"
        return 1
    fi
}

# Assert output does not contain string
assert_output_not_contains() {
    local command="$1"
    local unexpected="$2"
    local description="${3:-Output should not contain: $unexpected}"
    
    local output=$(get_vm_output "$command")
    if echo "$output" | grep -q "$unexpected"; then
        echo -e "${RED}✗ $description${NC}"
        echo "  Output: $output"
        return 1
    else
        echo -e "${GREEN}✓ $description${NC}"
        return 0
    fi
}

# Assert service is enabled
assert_service_enabled() {
    local service="$1"
    local description="${2:-Service should be enabled: $service}"
    
    case "$service" in
        postgresql)
            assert_command_succeeds "which psql" "$description"
            ;;
        redis)
            assert_command_succeeds "which redis-cli" "$description"
            ;;
        mongodb)
            assert_command_succeeds "which mongosh" "$description"
            ;;
        docker)
            assert_command_succeeds "which docker" "$description"
            ;;
        *)
            echo -e "${YELLOW}⚠ Unknown service: $service${NC}"
            return 1
            ;;
    esac
}

# Assert service is not enabled
assert_service_not_enabled() {
    local service="$1"
    local description="${2:-Service should not be enabled: $service}"
    
    case "$service" in
        postgresql)
            assert_command_fails "which psql" "$description"
            ;;
        redis)
            assert_command_fails "which redis-cli" "$description"
            ;;
        mongodb)
            assert_command_fails "which mongosh" "$description"
            ;;
        docker)
            assert_command_fails "which docker" "$description"
            ;;
        *)
            echo -e "${YELLOW}⚠ Unknown service: $service${NC}"
            return 1
            ;;
    esac
}

# Run a test and track results
run_test() {
    local test_name="$1"
    local test_function="$2"
    
    echo -e "\n${BLUE}Running test: $test_name${NC}"
    
    if $test_function; then
        echo -e "${GREEN}✓ Test passed: $test_name${NC}"
        return 0
    else
        echo -e "${RED}✗ Test failed: $test_name${NC}"
        return 1
    fi
}

# Generate a test report
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
    
    if [ $failed -eq 0 ]; then
        echo -e "\n${GREEN}✓ All tests passed!${NC}"
        return 0
    else
        echo -e "\n${RED}✗ Some tests failed${NC}"
        return 1
    fi
}