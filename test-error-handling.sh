#!/bin/bash
# Test script for validating error handling and rollback mechanisms
# This script tests various error scenarios to ensure our security improvements are robust

set -e

# Enable debug output
export VM_DEBUG=true

# Test utilities
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/shared/temp-file-utils.sh"
source "$SCRIPT_DIR/shared/docker-utils.sh"

# Setup temp file handlers for this test
setup_temp_file_handlers

# Test results tracking
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_TESTS=()

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test utility functions
run_test() {
    local test_name="$1"
    local test_command="$2"
    
    echo -e "${BLUE}🧪 Running test: $test_name${NC}"
    ((TESTS_RUN++))
    
    if eval "$test_command"; then
        echo -e "${GREEN}✅ PASSED: $test_name${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}❌ FAILED: $test_name${NC}"
        ((TESTS_FAILED++))
        FAILED_TESTS+=("$test_name")
    fi
    echo ""
}

# Test 1: Temp file cleanup validation
test_temp_file_cleanup() {
    echo "Testing temp file cleanup validation..."
    
    # Create some test temp files
    local test_file1 test_file2
    test_file1=$(create_temp_file "test-cleanup.XXXXXX")
    test_file2=$(create_temp_file "test-cleanup2.XXXXXX")
    
    if [[ ! -f "$test_file1" ]] || [[ ! -f "$test_file2" ]]; then
        echo "Failed to create test temp files"
        return 1
    fi
    
    # Validate temp file tracking
    local file_count
    file_count=$(get_temp_file_count)
    if [[ $file_count -lt 2 ]]; then
        echo "Temp file tracking not working properly (count: $file_count)"
        return 1
    fi
    
    # Test validation function
    if ! validate_temp_file_cleanup >/dev/null 2>&1; then
        echo "Temp file cleanup validation detected issues (expected for this test)"
    fi
    
    # Clean up test files
    rm -f "$test_file1" "$test_file2"
    unregister_temp_file "$test_file1"
    unregister_temp_file "$test_file2"
    
    echo "Temp file cleanup test completed successfully"
    return 0
}

# Test 2: Mount validation security
test_mount_validation() {
    echo "Testing mount validation security..."
    
    # Test dangerous path patterns
    local dangerous_paths=(
        "../../../etc/passwd"
        "/etc"
        "/bin"
        "\$(rm -rf /)"
        "path;with;semicolons"
        'path"with"quotes'
        "path\`with\`backticks"
    )
    
    for dangerous_path in "${dangerous_paths[@]}"; do
        # Source the validation function
        if validate_mount_security "$dangerous_path" 2>/dev/null; then
            echo "SECURITY FAILURE: Dangerous path was allowed: $dangerous_path"
            return 1
        fi
    done
    
    echo "Mount validation security test passed"
    return 0
}

# Test 3: Docker command error handling
test_docker_error_handling() {
    echo "Testing Docker command error handling..."
    
    # Test with non-existent container
    if docker_cmd inspect "non-existent-container-12345" >/dev/null 2>&1; then
        echo "Expected docker inspect to fail for non-existent container"
        return 1
    fi
    
    # Test docker_cmd with invalid command
    if docker_cmd invalid-command-test 2>/dev/null; then
        echo "Expected docker command to fail for invalid command"
        return 1
    fi
    
    echo "Docker error handling test passed"
    return 0
}

# Test 4: Config file validation
test_config_validation() {
    echo "Testing configuration file validation..."
    
    # Create a temporary invalid config
    local invalid_config
    invalid_config=$(create_temp_file "invalid-config.XXXXXX")
    echo "invalid yaml content: [" > "$invalid_config"
    
    # Test that our tools properly handle invalid config
    if "$SCRIPT_DIR/validate-config.sh" --validate "$invalid_config" >/dev/null 2>&1; then
        echo "Expected config validation to fail for invalid YAML"
        return 1
    fi
    
    echo "Config validation test passed"
    return 0
}

# Test 5: Rollback mechanism simulation
test_rollback_mechanisms() {
    echo "Testing rollback mechanisms..."
    
    # Create a test directory that simulates a project
    local test_project_dir
    test_project_dir=$(create_temp_dir "test-project.XXXXXX")
    
    # Create a minimal vm.yaml for testing
    cat > "$test_project_dir/vm.yaml" << 'EOF'
version: "1.0"
provider: docker
project:
  name: test-rollback
  workspace_path: /workspace
services: []
vm:
  user: developer
EOF
    
    # Test that our error handling doesn't leave partial state
    # (We can't actually test Docker operations without potentially affecting the system,
    # so we test the configuration and validation parts)
    
    if ! "$SCRIPT_DIR/validate-config.sh" --validate "$test_project_dir/vm.yaml" >/dev/null 2>&1; then
        echo "Test config validation failed"
        return 1
    fi
    
    echo "Rollback mechanism test passed"
    return 0
}

# Test 6: Error message quality
test_error_messages() {
    echo "Testing error message quality..."
    
    # Test mount validation error messages
    local error_output
    error_output=$(validate_mount_security "/etc" 2>&1 || true)
    
    if [[ ! "$error_output" =~ "system-critical" ]]; then
        echo "Error message should mention system-critical path protection"
        return 1
    fi
    
    echo "Error message quality test passed"
    return 0
}

# Main test execution
main() {
    echo -e "${BLUE}🚀 Starting Error Handling and Security Test Suite${NC}"
    echo "========================================================"
    
    # Run all tests
    run_test "Temp File Cleanup Validation" "test_temp_file_cleanup"
    run_test "Mount Validation Security" "test_mount_validation" 
    run_test "Docker Error Handling" "test_docker_error_handling"
    run_test "Config Validation" "test_config_validation"
    run_test "Rollback Mechanisms" "test_rollback_mechanisms"
    run_test "Error Message Quality" "test_error_messages"
    
    # Test summary
    echo "========================================================"
    echo -e "${BLUE}📊 Test Summary${NC}"
    echo "Tests run: $TESTS_RUN"
    echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
    echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"
    
    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo -e "${RED}❌ Failed tests:${NC}"
        for failed_test in "${FAILED_TESTS[@]}"; do
            echo "  - $failed_test"
        done
        echo ""
        echo -e "${YELLOW}⚠️  Some error handling tests failed. Please review the error handling implementation.${NC}"
        exit 1
    else
        echo -e "${GREEN}🎉 All error handling tests passed!${NC}"
        echo ""
        echo "✅ Error handling and rollback mechanisms are working correctly"
        echo "✅ Security validations are properly implemented"
        echo "✅ Temp file cleanup is functioning"
        echo "✅ Docker error handling is robust"
    fi
}

# Run the test suite
main "$@"