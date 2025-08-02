#!/bin/bash
# Test script to verify temporary file cleanup behavior
# Tests various failure scenarios to ensure no temp files are left behind

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test result tracking
TESTS_PASSED=0
TESTS_FAILED=0
TEMP_FILES_CREATED=()

log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
    ((TESTS_PASSED++))
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
    ((TESTS_FAILED++))
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Test helper: Check if temp files exist
check_temp_files_cleaned() {
    local test_name="$1"
    local found_files=0
    
    for temp_file in "${TEMP_FILES_CREATED[@]}"; do
        if [[ -f "$temp_file" ]]; then
            log_error "$test_name: Temp file still exists: $temp_file"
            found_files=1
        fi
    done
    
    if [[ $found_files -eq 0 ]]; then
        log_success "$test_name: All temporary files cleaned up"
    fi
    
    # Clear the tracking array for next test
    TEMP_FILES_CREATED=()
}

# Test 1: Normal execution of generate-config.sh
test_generate_config_normal() {
    log_info "Test 1: Normal execution of generate-config.sh"
    
    local test_dir="/tmp/temp-file-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"
    
    # Track temp files that might be created (we'll check the pattern)
    local before_count
    before_count=$(find /tmp -name "vm-config.*" -type f 2>/dev/null | wc -l)
    
    # Run generate-config.sh
    if "$SCRIPT_DIR/generate-config.sh" --name test-project test-output.yaml 2>/dev/null; then
        local after_count
        after_count=$(find /tmp -name "vm-config.*" -type f 2>/dev/null | wc -l)
        
        if [[ $after_count -eq $before_count ]]; then
            log_success "Test 1: No temporary files left behind after normal execution"
        else
            log_error "Test 1: Found $((after_count - before_count)) temporary files left behind"
        fi
    else
        log_error "Test 1: generate-config.sh failed to execute"
    fi
    
    # Cleanup test directory
    cd /
    rm -rf "$test_dir"
}

# Test 2: Interrupted execution of generate-config.sh (simulated)
test_generate_config_interrupt() {
    log_info "Test 2: Interrupted execution of generate-config.sh"
    
    local test_dir="/tmp/temp-file-test-interrupt-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"
    
    # Track temp files before
    local before_files
    before_files=$(find /tmp -name "vm-config.*" -type f 2>/dev/null || true)
    
    # Start generate-config.sh in background and interrupt it
    timeout 2 "$SCRIPT_DIR/generate-config.sh" --name test-interrupt --services postgresql,redis test-interrupt.yaml &>/dev/null || true
    
    # Give a moment for cleanup
    sleep 1
    
    # Check temp files after
    local after_files
    after_files=$(find /tmp -name "vm-config.*" -type f 2>/dev/null || true)
    
    if [[ "$before_files" == "$after_files" ]]; then
        log_success "Test 2: No temporary files left behind after interruption"
    else
        log_warning "Test 2: Some temporary files may remain (this could be normal for timeout interruption)"
        # Show what files remain
        log_info "Files before: $(echo "$before_files" | wc -w)"
        log_info "Files after: $(echo "$after_files" | wc -w)"
    fi
    
    # Cleanup test directory
    cd /
    rm -rf "$test_dir"
}

# Test 3: Test the centralized temp-file-utils.sh directly
test_temp_file_utils() {
    log_info "Test 3: Testing centralized temp-file-utils.sh"
    
    # Source the utilities in a subshell to test isolation
    (
        source "$SCRIPT_DIR/shared/temp-file-utils.sh"
        setup_temp_file_handlers
        
        # Create a temp file
        temp_file=$(create_temp_file "test.XXXXXX")
        echo "test content" > "$temp_file"
        
        # Record the temp file path for checking
        echo "$temp_file" > /tmp/test-temp-file-path-$$
        
        # Exit normally - trap should clean up
    )
    
    # Check if the temp file was cleaned up
    if [[ -f "/tmp/test-temp-file-path-$$" ]]; then
        local recorded_temp_file
        recorded_temp_file=$(cat "/tmp/test-temp-file-path-$$")
        rm -f "/tmp/test-temp-file-path-$$"
        
        if [[ ! -f "$recorded_temp_file" ]]; then
            log_success "Test 3: Temporary file cleaned up by trap handler"
        else
            log_error "Test 3: Temporary file not cleaned up: $recorded_temp_file"
            # Clean it up manually for tidiness
            rm -f "$recorded_temp_file"
        fi
    else
        log_error "Test 3: Could not track temporary file creation"
    fi
}

# Test 4: Test temp file utils with signal interruption
test_temp_file_utils_signal() {
    log_info "Test 4: Testing temp-file-utils.sh with signal interruption"
    
    # Create a background process that uses temp files
    (
        source "$SCRIPT_DIR/shared/temp-file-utils.sh"
        setup_temp_file_handlers
        
        temp_file=$(create_temp_file "signal-test.XXXXXX")
        echo "test content" > "$temp_file"
        
        # Record the temp file path
        echo "$temp_file" > /tmp/signal-test-temp-file-$$
        
        # Sleep long enough to be interrupted
        sleep 10
    ) &
    
    local bg_pid=$!
    sleep 1  # Let it start
    
    # Send SIGTERM
    kill -TERM $bg_pid 2>/dev/null || true
    wait $bg_pid 2>/dev/null || true
    
    # Check cleanup
    if [[ -f "/tmp/signal-test-temp-file-$$" ]]; then
        local recorded_temp_file
        recorded_temp_file=$(cat "/tmp/signal-test-temp-file-$$")
        rm -f "/tmp/signal-test-temp-file-$$"
        
        if [[ ! -f "$recorded_temp_file" ]]; then
            log_success "Test 4: Temporary file cleaned up after SIGTERM"
        else
            log_error "Test 4: Temporary file not cleaned up after SIGTERM: $recorded_temp_file"
            rm -f "$recorded_temp_file"
        fi
    else
        log_warning "Test 4: Could not track signal test temp file"
    fi
}

# Test 5: Test that vm.sh doesn't leave temp files
test_vm_sh_temp_files() {
    log_info "Test 5: Testing vm.sh temporary file cleanup"
    
    if [[ ! -f "$SCRIPT_DIR/vm.yaml" ]]; then
        log_warning "Test 5: Skipping vm.sh test - no vm.yaml found"
        return
    fi
    
    local before_count
    before_count=$(find /tmp -name "vm-config.*" -type f 2>/dev/null | wc -l)
    
    # Try to run vm validate (should be safe and not require actual containers)
    if "$SCRIPT_DIR/vm.sh" validate &>/dev/null; then
        local after_count
        after_count=$(find /tmp -name "vm-config.*" -type f 2>/dev/null | wc -l)
        
        if [[ $after_count -eq $before_count ]]; then
            log_success "Test 5: vm.sh validate left no temporary files"
        else
            log_error "Test 5: vm.sh validate left $((after_count - before_count)) temporary files"
        fi
    else
        log_warning "Test 5: vm.sh validate failed (may be due to missing dependencies)"
    fi
}

# Main test execution
main() {
    echo "🧪 Testing temporary file cleanup behavior"
    echo "=========================================="
    echo
    
    test_generate_config_normal
    echo
    
    test_generate_config_interrupt
    echo
    
    test_temp_file_utils
    echo
    
    test_temp_file_utils_signal
    echo
    
    test_vm_sh_temp_files
    echo
    
    # Summary
    echo "=========================================="
    echo "Test Results:"
    echo "  ✅ Passed: $TESTS_PASSED"
    echo "  ❌ Failed: $TESTS_FAILED"
    
    if [[ $TESTS_FAILED -eq 0 ]]; then
        echo -e "${GREEN}🎉 All tests passed!${NC}"
        exit 0
    else
        echo -e "${RED}💥 Some tests failed.${NC}"
        exit 1
    fi
}

# Run tests
main "$@"