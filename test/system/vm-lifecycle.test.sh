#!/bin/bash
# VM Lifecycle System Tests - Extracted from main test.sh
# Tests VM operations: minimal-boot, vm-status, vm-exec, vm-lifecycle, vm-reload

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG_DIR="$SCRIPT_DIR/../configs"

# Source shared utilities
source "$SCRIPT_DIR/../../shared/docker-utils.sh"

# Test state
TEST_DIR=""
TEST_NAME=""
TEST_PROVIDER=""
CLEANUP_COMMANDS=()

# Test results
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
FAILED_TEST_NAMES=()

# ============================================================================
# Test Framework Helper Functions
# ============================================================================

# Skip test with helpful message about Docker access
skip_docker_test() {
    local test_name="$1"
    echo -e "${YELLOW}⚠ Skipping $test_name: Docker access unavailable${NC}"
    echo -e "${YELLOW}  To enable this test:${NC}"
    echo -e "${YELLOW}    1. Add user to docker group: sudo usermod -aG docker \$USER${NC}"
    echo -e "${YELLOW}    2. Restart session: newgrp docker${NC}"
    echo -e "${YELLOW}    3. Or ensure Docker daemon is running and accessible${NC}"
    return 0
}

# Initialize test environment
setup_test_env() {
    local test_name="$1"
    local provider="${2:-docker}"

    local TEST_NAME="$test_name"
    local TEST_PROVIDER="$provider"
    export TEST_NAME
    export TEST_PROVIDER
    TEST_DIR="$SCRIPT_DIR/../.test_artifacts/vm-test-${test_name}-$$"

    # Ensure test runs directory exists
    mkdir -p "$SCRIPT_DIR/../.test_artifacts"
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
        # Execute each command safely in a subshell
        bash -c "$(printf '%q' "$cmd")" 2>/dev/null || true
    done

    # Destroy VM if it exists
    if [[ -f "$TEST_DIR/vm.yaml" ]]; then
        cd "$TEST_DIR"
        # Destroy VM without sudo
        vm destroy -f 2>/dev/null || true

        # Extract project name and ensure container is removed
        local project_name
        project_name=$(yq '.project.name' vm.yaml 2>/dev/null | tr -cd '[:alnum:]')
        if [[ -n "$project_name" ]]; then
            local container_name="${project_name}-dev"
            # Force stop and remove container with both docker and sudo docker
            docker stop "$container_name" 2>/dev/null || sudo docker stop "$container_name" 2>/dev/null || true
            docker rm "$container_name" 2>/dev/null || sudo docker rm "$container_name" 2>/dev/null || true
        fi
    fi

    # Remove test directory
    rm -rf "$TEST_DIR" || true
}

# Register a cleanup command
register_cleanup() {
    CLEANUP_COMMANDS+=("$1")
}

# Create a test VM with given config
create_test_vm() {
    local config_path="$1"
    local timeout="${2:-600}"  # 10 minute default timeout

    echo -e "${BLUE}Creating test VM with config: $config_path${NC}"

    # Copy config to test directory
    if [[ -f "$config_path" ]]; then
        cp "$config_path" "$TEST_DIR/vm.yaml"
    else
        echo -e "${RED}Config file not found: $config_path${NC}"
        return 1
    fi

    # Pre-emptively clean up any existing container with the same name
    local project_name
    project_name=$(yq '.project.name' "$TEST_DIR/vm.yaml" 2>/dev/null | tr -cd '[:alnum:]')
    if [[ -n "$project_name" ]]; then
        local container_name="${project_name}-dev"
        echo -e "${BLUE}Cleaning up any existing container: $container_name${NC}"
        docker stop "$container_name" 2>/dev/null || sudo docker stop "$container_name" 2>/dev/null || true
        docker rm "$container_name" 2>/dev/null || sudo docker rm "$container_name" 2>/dev/null || true
    fi

    # Start VM with timeout
    cd "$TEST_DIR"
    # Try without sudo first since docker-compose is now available
    if ! (cd "$SCRIPT_DIR/.." && npm link && cd "$TEST_DIR" && timeout "$timeout" vm create); then
        echo -e "${RED}Failed to create VM within ${timeout}s${NC}"
        return 1
    fi

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
    # Execute command in VM
    vm exec "$command"
    local exit_code=$?

    if [[ "$expected_exit" != "any" ]] && [[ $exit_code -ne "$expected_exit" ]]; then
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
    # Check VM status directly
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

# Run a test and track results
run_test() {
    local test_name="$1"
    local test_function="$2"

    echo -e "\n${BLUE}Running test: $test_name${NC}"

    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    # Run test in a subshell to isolate failures
    if (
        set -e
        setup_test_env "${test_name}" "docker"
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

# Generate test configurations
generate_configs() {
    echo "Generating test configurations..."

    # Create configs directory
    mkdir -p "$CONFIG_DIR"

    # Generate minimal config
    cat > "$CONFIG_DIR/minimal.yaml" << EOF
project:
  name: test-minimal
  hostname: dev.minimal.local
  workspace_path: /workspace
provider: docker
terminal:
  emoji: 🧪
  username: test-dev
services: {}
aliases: {}
EOF

    echo "✓ Test configurations generated"
}

# ============================================================================
# VM Lifecycle System Tests
# ============================================================================

# Test that VM boots with minimal config
test_minimal_boot() {
    echo "Testing VM boot with minimal configuration..."

    # Check Docker access before attempting VM creation
    if ! check_docker_access; then
        skip_docker_test "minimal VM boot test"
        return 0
    fi

    # Create VM with minimal config - with shorter timeout for debugging
    create_test_vm "$CONFIG_DIR/minimal.yaml" 180 || return 1

    # If we get here, the VM started successfully
    echo -e "${GREEN}✓ VM created successfully${NC}"

    # Basic checks - but let's simplify to avoid more recursion
    cd "$TEST_DIR"
    if vm status 2>&1 | grep -q -E "(running|up|started)"; then
        echo -e "${GREEN}✓ VM is running${NC}"
    else
        echo -e "${YELLOW}⚠ VM status unclear, but creation succeeded${NC}"
    fi
}

# Test vm status command
test_vm_status() {
    echo "Testing vm status command..."

    # Check Docker access before attempting VM creation
    if ! check_docker_access; then
        skip_docker_test "VM status test"
        return 0
    fi

    create_test_vm "$CONFIG_DIR/minimal.yaml" || return 1

    # Check status when running
    cd "$TEST_DIR"
    local status_output
    status_output=$(vm status 2>&1)

    if echo "$status_output" | grep -q "running"; then
        echo -e "${GREEN}✓ vm status shows running state${NC}"
    else
        echo -e "${RED}✗ vm status should show running state${NC}"
        echo "Output: $status_output"
        return 1
    fi

    # Halt VM
    vm halt || return 1
    sleep 5

    # Check status when stopped
    status_output=$(vm status 2>&1)
    if echo "$status_output" | grep -q -E "(stopped|poweroff|halted)"; then
        echo -e "${GREEN}✓ vm status shows stopped state${NC}"
    else
        echo -e "${RED}✗ vm status should show stopped state${NC}"
        echo "Output: $status_output"
        return 1
    fi
}

# Test vm exec command
test_vm_exec() {
    echo "Testing vm exec command..."

    # Check Docker access before attempting VM creation
    if ! check_docker_access; then
        skip_docker_test "VM exec test"
        return 0
    fi

    create_test_vm "$CONFIG_DIR/minimal.yaml" || return 1

    cd "$TEST_DIR"

    # Test simple command
    local output
    output=$(vm exec "echo hello" 2>&1)
    if echo "$output" | grep -q "hello"; then
        echo -e "${GREEN}✓ vm exec runs commands${NC}"
    else
        echo -e "${RED}✗ vm exec should run commands${NC}"
        echo "Output: $output"
        return 1
    fi

    # Test command with exit code
    if vm exec "exit 0"; then
        echo -e "${GREEN}✓ vm exec preserves exit codes${NC}"
    else
        echo -e "${RED}✗ vm exec should preserve exit codes${NC}"
        return 1
    fi
}

# Test VM creation and destruction
test_vm_lifecycle() {
    echo "Testing VM lifecycle..."

    # Check Docker access before attempting VM operations
    if ! check_docker_access; then
        skip_docker_test "VM lifecycle test"
        return 0
    fi

    create_test_vm "$CONFIG_DIR/minimal.yaml" || return 1

    cd "$TEST_DIR"

    # Test VM is running
    assert_vm_running

    # Test we can execute commands
    assert_command_succeeds "echo 'lifecycle test'" "Execute command in running VM"

    # Test VM halt
    vm halt || return 1
    sleep 5
    assert_vm_stopped

    # Test VM restart
    vm create || return 1
    sleep 5
    assert_vm_running

    # Test VM destroy
    vm destroy -f || return 1

    # Check VM is gone
    if vm status 2>&1 | grep -q -E "(not created|not found|no such)"; then
        echo -e "${GREEN}✓ VM destroyed successfully${NC}"
    else
        echo -e "${RED}✗ VM should be destroyed${NC}"
        return 1
    fi
}

# Test VM reload
test_vm_reload() {
    echo "Testing VM reload..."

    # Check Docker access before attempting VM creation
    if ! check_docker_access; then
        skip_docker_test "VM reload test"
        return 0
    fi

    create_test_vm "$CONFIG_DIR/minimal.yaml" || return 1

    cd "$TEST_DIR"

    # Create a test file in VM
    vm exec "echo 'before reload' > /tmp/reload-test"

    # Modify config (add an alias)
    yq '.aliases.testreload = "echo reload-success"' vm.yaml -o yaml > vm.yaml.tmp
    mv vm.yaml.tmp vm.yaml

    # Reload VM
    vm reload || return 1
    sleep 10  # Give time for provisioning

    # Check VM is still running
    assert_vm_running

    # Check new alias is available
    if vm exec "source ~/.zshrc && type testreload" 2>&1 | grep -q "alias"; then
        echo -e "${GREEN}✓ vm reload applies config changes${NC}"
    else
        echo -e "${RED}✗ vm reload should apply config changes${NC}"
        return 1
    fi
}

# ============================================================================
# Main Test Runner
# ============================================================================

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

    if [[ "$failed" -eq 0 ]]; then
        echo -e "\n${GREEN}✓ All tests passed!${NC}"
        return 0
    else
        echo -e "\n${RED}✗ Some tests failed${NC}"
        return 1
    fi
}

# Main execution
main() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}VM Lifecycle System Tests${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo "Testing provider: docker"
    echo ""

    # Generate test configurations
    echo -e "\n${BLUE}Generating test configurations...${NC}"
    generate_configs

    # Make vm.sh available as 'vm' command
    export PATH="$SCRIPT_DIR/..:$PATH"

    # Run VM lifecycle tests
    run_test "minimal-boot" test_minimal_boot
    run_test "vm-status" test_vm_status
    run_test "vm-exec" test_vm_exec
    run_test "vm-lifecycle" test_vm_lifecycle
    run_test "vm-reload" test_vm_reload

    # Generate final report
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    generate_test_report $PASSED_TESTS $FAILED_TESTS

    # Show failed tests if any
    if [[ ${#FAILED_TEST_NAMES[@]} -gt 0 ]]; then
        echo -e "\n${RED}Failed tests:${NC}"
        for test_name in "${FAILED_TEST_NAMES[@]}"; do
            echo -e "  ${RED}✗ $test_name${NC}"
        done
    fi

    # Exit with appropriate code
    [ $FAILED_TESTS -eq 0 ]
}

# Parse command line arguments
case "${1:-}" in
    --help|-h)
        echo "VM Lifecycle System Test Suite"
        echo ""
        echo "Usage: $0 [test-name]"
        echo ""
        echo "Available tests:"
        echo "  minimal-boot     Test VM creation with minimal config"
        echo "  vm-status        Test vm status command"
        echo "  vm-exec          Test vm exec command"
        echo "  vm-lifecycle     Test full VM lifecycle operations"
        echo "  vm-reload        Test VM reload functionality"
        echo ""
        echo "Examples:"
        echo "  $0               Run all VM lifecycle tests"
        echo "  $0 minimal-boot  Run only minimal boot test"
        exit 0
        ;;
    minimal-boot)
        generate_configs
        export PATH="$SCRIPT_DIR/..:$PATH"
        run_test "minimal-boot" test_minimal_boot
        ;;
    vm-status)
        generate_configs
        export PATH="$SCRIPT_DIR/..:$PATH"
        run_test "vm-status" test_vm_status
        ;;
    vm-exec)
        generate_configs
        export PATH="$SCRIPT_DIR/..:$PATH"
        run_test "vm-exec" test_vm_exec
        ;;
    vm-lifecycle)
        generate_configs
        export PATH="$SCRIPT_DIR/..:$PATH"
        run_test "vm-lifecycle" test_vm_lifecycle
        ;;
    vm-reload)
        generate_configs
        export PATH="$SCRIPT_DIR/..:$PATH"
        run_test "vm-reload" test_vm_reload
        ;;
    "")
        main
        ;;
    *)
        echo "Unknown test: $1"
        echo "Run $0 --help for available options"
        exit 1
        ;;
esac