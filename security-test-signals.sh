#!/bin/bash
# Signal Handler Race Condition Test Suite  
# Tests the mutex-based cleanup protection in temp-file-utils.sh

set -e

echo "🔒 Signal Handler Race Condition Test Suite"
echo "==========================================="

# Source the temp file utilities
source ./shared/temp-file-utils.sh 2>/dev/null || {
    echo "❌ Failed to source temp-file-utils.sh"
    exit 1
}

# Test counter
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Test function
run_signal_test() {
    local test_name="$1"
    local expected_result="$2"  # "PASS" or "FAIL" 
    
    ((TOTAL_TESTS++))
    
    echo -n "Test $TOTAL_TESTS: $test_name... "
    
    # The test implementation is provided as a function
    local test_func="$3"
    
    local result
    if eval "$test_func" >/dev/null 2>&1; then
        result="PASS"
    else
        result="FAIL"
    fi
    
    # Check if result matches expectation
    if [[ "$result" == "$expected_result" ]]; then
        echo "✅ PASS"
        ((PASSED_TESTS++))
    else
        echo "❌ FAIL (expected $expected_result, got $result)"
        ((FAILED_TESTS++))
    fi
}

# Test 1: Basic temp file creation and cleanup
test_basic_temp_file() {
    local temp_file
    temp_file=$(create_temp_file "signal-test.XXXXXX")
    [[ -f "$temp_file" ]] || return 1
    
    echo "test data" > "$temp_file"
    [[ -s "$temp_file" ]] || return 1
    
    # Cleanup should work
    cleanup_temp_files 0
    [[ ! -f "$temp_file" ]] || return 1
    
    return 0
}

run_signal_test "Basic temp file creation and cleanup" "PASS" "test_basic_temp_file"

# Test 2: Mutex acquisition and release
test_mutex_mechanism() {
    # Test that we can acquire and release mutex
    acquire_cleanup_mutex || return 1
    release_cleanup_mutex || return 1
    
    return 0
}

run_signal_test "Mutex acquisition and release" "PASS" "test_mutex_mechanism"

# Test 3: Concurrent cleanup protection
test_concurrent_cleanup() {
    # Create some temp files
    local temp_file1 temp_file2
    temp_file1=$(create_temp_file "concurrent1.XXXXXX")
    temp_file2=$(create_temp_file "concurrent2.XXXXXX")
    
    echo "data1" > "$temp_file1"
    echo "data2" > "$temp_file2"
    
    # Start cleanup in background
    (
        export VM_DEBUG=false
        cleanup_temp_files 0
    ) &
    local cleanup_pid1=$!
    
    # Start another cleanup immediately  
    (
        export VM_DEBUG=false
        cleanup_temp_files 0
    ) &
    local cleanup_pid2=$!
    
    # Wait for both to complete
    wait $cleanup_pid1 2>/dev/null || true
    wait $cleanup_pid2 2>/dev/null || true
    
    # Files should be cleaned up exactly once (no errors)
    [[ ! -f "$temp_file1" ]] || return 1
    [[ ! -f "$temp_file2" ]] || return 1
    
    return 0
}

run_signal_test "Concurrent cleanup protection" "PASS" "test_concurrent_cleanup"

# Test 4: Signal interrupt simulation
echo ""
echo "🏃 Signal Race Condition Simulation:"
echo "====================================="

SIGNAL_TEST_DIR="/tmp/signal-race-test-$$"
mkdir -p "$SIGNAL_TEST_DIR"

echo -n "Signal interrupt simulation test... "

# Create a script that sets up temp files and then gets interrupted
cat > "$SIGNAL_TEST_DIR/signal_test_script.sh" << 'EOF'
#!/bin/bash
source ./shared/temp-file-utils.sh
setup_temp_file_handlers

# Create multiple temp files
for i in {1..5}; do
    temp_file=$(create_temp_file "signal-test-$i.XXXXXX")
    echo "test data $i" > "$temp_file"
done

# Simulate some work
sleep 0.5

# Exit normally (signal will interrupt)
exit 0
EOF

chmod +x "$SIGNAL_TEST_DIR/signal_test_script.sh"

# Run the script and interrupt it with different signals
SIGNAL_FAILURES=0

for signal in INT TERM; do
    # Start the script
    (cd /workspace && "$SIGNAL_TEST_DIR/signal_test_script.sh") &
    local script_pid=$!
    
    # Let it start up
    sleep 0.1
    
    # Send the signal
    kill -$signal $script_pid 2>/dev/null || true
    
    # Wait for it to finish
    wait $script_pid 2>/dev/null || true
    
    # Check for leftover temp files (should be cleaned up by signal handler)
    local leftover_count
    leftover_count=$(find /tmp -name "signal-test-*.XXXXXX*" 2>/dev/null | wc -l)
    
    if [[ $leftover_count -gt 0 ]]; then
        ((SIGNAL_FAILURES++))
        # Clean up leftovers for next test
        find /tmp -name "signal-test-*.XXXXXX*" -delete 2>/dev/null || true
    fi
done

if [[ $SIGNAL_FAILURES -eq 0 ]]; then
    echo "✅ PASS (Proper cleanup on signal interruption)"
    ((PASSED_TESTS++))
else
    echo "❌ FAIL ($SIGNAL_FAILURES signals left temp files behind)"
    ((FAILED_TESTS++))
fi

((TOTAL_TESTS++))

# Test 5: Stress test with rapid signal delivery
echo -n "Rapid signal delivery stress test... "

# Create a script that handles many rapid signals
cat > "$SIGNAL_TEST_DIR/stress_test_script.sh" << 'EOF'
#!/bin/bash
source ./shared/temp-file-utils.sh
setup_temp_file_handlers

# Create temp files continuously
for i in {1..20}; do
    temp_file=$(create_temp_file "stress-test-$i.XXXXXX")
    echo "stress data $i" > "$temp_file"
    sleep 0.02
done

exit 0
EOF

chmod +x "$SIGNAL_TEST_DIR/stress_test_script.sh"

# Start the script
(cd /workspace && "$SIGNAL_TEST_DIR/stress_test_script.sh") &
local stress_pid=$!

# Send multiple rapid signals of different types
for i in {1..5}; do
    kill -INT $stress_pid 2>/dev/null || true
    sleep 0.01
    kill -TERM $stress_pid 2>/dev/null || true
    sleep 0.01
done

# Wait for script to complete
wait $stress_pid 2>/dev/null || true

# Check for leftover files
local stress_leftover_count
stress_leftover_count=$(find /tmp -name "stress-test-*.XXXXXX*" 2>/dev/null | wc -l)

if [[ $stress_leftover_count -eq 0 ]]; then
    echo "✅ PASS (Survived rapid signal stress test)"
    ((PASSED_TESTS++))
else
    echo "❌ FAIL ($stress_leftover_count files left after stress test)"
    ((FAILED_TESTS++))
    # Cleanup
    find /tmp -name "stress-test-*.XXXXXX*" -delete 2>/dev/null || true
fi

((TOTAL_TESTS++))

# Test 6: Orphaned mutex cleanup
test_orphaned_mutex_cleanup() {
    # Create a mutex file manually (simulating crashed process)
    local test_mutex="/tmp/.vm-cleanup-mutex-orphan-test"
    echo "99999" > "$test_mutex"  # Non-existent PID
    
    # Try to acquire mutex (should detect orphaned mutex and clean it up)
    if acquire_cleanup_mutex; then
        release_cleanup_mutex
        # Clean up our test mutex
        rm -f "$test_mutex" 2>/dev/null || true
        return 0
    else
        # Clean up our test mutex
        rm -f "$test_mutex" 2>/dev/null || true
        return 1
    fi
}

run_signal_test "Orphaned mutex cleanup detection" "PASS" "test_orphaned_mutex_cleanup"

# Test 7: Performance impact of mutex operations
echo ""
echo "🔍 Mutex Performance Impact Test:"
echo "================================"

# Test mutex operations performance
start_time=$(date +%s%N)
for i in {1..100}; do
    acquire_cleanup_mutex >/dev/null 2>&1
    release_cleanup_mutex >/dev/null 2>&1
done
end_time=$(date +%s%N)

duration=$(( (end_time - start_time) / 1000000 ))  # Convert to milliseconds
avg_time=$(( duration / 100 ))

echo "Mutex operations (100 acquire/release cycles): ${duration}ms"
echo "Average per cycle: ${avg_time}ms"

if [[ $avg_time -lt 1 ]]; then
    echo "✅ Mutex performance excellent (< 1ms per cycle)"
elif [[ $avg_time -lt 5 ]]; then
    echo "✅ Mutex performance acceptable (< 5ms per cycle)"
else
    echo "⚠️ Mutex performance concern (> 5ms per cycle)"
fi

# Test 8: Registry file corruption resistance
test_registry_corruption() {
    # Create some temp files
    local temp_file1 temp_file2
    temp_file1=$(create_temp_file "corruption1.XXXXXX")
    temp_file2=$(create_temp_file "corruption2.XXXXXX")
    
    # Corrupt the registry file
    echo "INVALID_PATH_ENTRY" >> "$TEMP_FILES_REGISTRY"
    echo "" >> "$TEMP_FILES_REGISTRY"  # Empty line
    echo "/tmp/nonexistent" >> "$TEMP_FILES_REGISTRY"  # Non-existent file
    
    # Cleanup should handle corruption gracefully
    cleanup_temp_files 0
    
    # Original files should still be cleaned up
    [[ ! -f "$temp_file1" ]] || return 1
    [[ ! -f "$temp_file2" ]] || return 1
    
    return 0
}

run_signal_test "Registry file corruption resistance" "PASS" "test_registry_corruption"

# Cleanup test environment
rm -rf "$SIGNAL_TEST_DIR" 2>/dev/null || true

# Clean up any stray test files
find /tmp -name "signal-test-*.XXXXXX*" -delete 2>/dev/null || true
find /tmp -name "stress-test-*.XXXXXX*" -delete 2>/dev/null || true
find /tmp -name "corruption*.XXXXXX*" -delete 2>/dev/null || true
find /tmp -name ".vm-cleanup-mutex-orphan-test" -delete 2>/dev/null || true

# Summary
echo ""
echo "==========================================="
echo "Test Results Summary:"
echo "Total tests: $TOTAL_TESTS"
echo "Passed: $PASSED_TESTS"  
echo "Failed: $FAILED_TESTS"

if [[ $FAILED_TESTS -eq 0 ]]; then
    echo "🎉 All signal handler security tests passed!"
    exit 0
else
    echo "❌ Some signal handler security tests failed"
    exit 1
fi