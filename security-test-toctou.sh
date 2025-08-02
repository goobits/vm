#!/bin/bash
# TOCTOU (Time-of-Check Time-of-Use) Attack Test Suite
# Tests the symlink race condition protection in vm.sh

set -e

echo "🔒 TOCTOU Security Validation Test Suite"
echo "========================================"

# Source the VM functions by setting skip flag and sourcing
SKIP_MAIN_EXECUTION=true
source ./vm.sh 2>/dev/null || {
    echo "❌ Failed to source vm.sh functions"
    exit 1
}

# Test counter
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Cleanup function
cleanup_test_files() {
    local base_dir="$1"
    if [[ -n "$base_dir" ]] && [[ -d "$base_dir" ]]; then
        rm -rf "$base_dir" 2>/dev/null || true
    fi
}

# Test function
run_toctou_test() {
    local test_name="$1"
    local expected_result="$2"  # "PASS" or "FAIL"
    shift 2
    
    ((TOTAL_TESTS++))
    
    echo -n "Test $TOTAL_TESTS: $test_name... "
    
    # Create test environment
    local test_base="/tmp/toctou-test-$$-$RANDOM"
    mkdir -p "$test_base"
    
    # Set up the test environment by running the provided commands
    local setup_result=0
    for cmd in "$@"; do
        if ! eval "$cmd" 2>/dev/null; then
            setup_result=1
            break
        fi
    done
    
    if [[ $setup_result -ne 0 ]]; then
        echo "❌ SETUP FAILED"
        cleanup_test_files "$test_base"
        ((FAILED_TESTS++))
        return
    fi
    
    # Run the validation and mount construction (disable strict error checking)
    local result
    set +e
    process_single_mount "$test_base/link" >/dev/null 2>&1
    local mount_exit_code=$?
    set -e
    
    if [[ $mount_exit_code -eq 0 ]]; then
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
    
    # Cleanup
    cleanup_test_files "$test_base"
}

# Test 1: Basic legitimate symlink (should pass)
run_toctou_test "Legitimate symlink to safe directory" "PASS" \
    "mkdir -p '$test_base/safe'" \
    "ln -s '$test_base/safe' '$test_base/link'"

# Test 2: Symlink to dangerous system directory (should fail)
run_toctou_test "Symlink to /etc (should fail)" "FAIL" \
    "ln -s '/etc' '$test_base/link'"

# Test 3: Symlink to root (should fail)
run_toctou_test "Symlink to root filesystem (should fail)" "FAIL" \
    "ln -s '/' '$test_base/link'"

# Test 4: Symlink chain attack (should fail)
run_toctou_test "Symlink chain to dangerous directory" "FAIL" \
    "ln -s '/etc/passwd' '$test_base/dangerous'" \
    "ln -s '$test_base/dangerous' '$test_base/link'"

# Test 5: Relative symlink escape attempt (should fail)
run_toctou_test "Relative symlink escape attempt" "FAIL" \
    "ln -s '../../../etc' '$test_base/link'"

# Test 6: Self-referencing symlink (should fail)
run_toctou_test "Self-referencing symlink" "FAIL" \
    "ln -s '$test_base/link' '$test_base/link'"

# Test 7: Circular symlink (should fail)
run_toctou_test "Circular symlink reference" "FAIL" \
    "ln -s '$test_base/link2' '$test_base/link'" \
    "ln -s '$test_base/link' '$test_base/link2'"

# Test 8: Race condition simulation
echo ""
echo "🏃 TOCTOU Race Condition Simulation:"
echo "===================================="

RACE_TEST_DIR="/tmp/toctou-race-test-$$"
mkdir -p "$RACE_TEST_DIR/safe"
mkdir -p "$RACE_TEST_DIR/dangerous"

# Create initial safe symlink
ln -s "$RACE_TEST_DIR/safe" "$RACE_TEST_DIR/racey-link"

echo -n "Race condition test (changing symlink target rapidly)... "

# Background process that changes the symlink target
(
    for i in {1..50}; do
        ln -sf "$RACE_TEST_DIR/safe" "$RACE_TEST_DIR/racey-link" 2>/dev/null
        sleep 0.01
        ln -sf "/etc" "$RACE_TEST_DIR/racey-link" 2>/dev/null
        sleep 0.01
    done
) &

RACE_PID=$!

# Test validation multiple times during the race
RACE_FAILURES=0
RACE_ATTEMPTS=20

for attempt in $(seq 1 $RACE_ATTEMPTS); do
    set +e
    process_single_mount "$RACE_TEST_DIR/racey-link" >/dev/null 2>&1
    local race_result=$?
    set -e
    
    if [[ $race_result -eq 0 ]]; then
        # If it passes, verify it's actually pointing to a safe location
        LINK_TARGET=$(readlink -f "$RACE_TEST_DIR/racey-link" 2>/dev/null)
        if [[ "$LINK_TARGET" == "/etc" ]] || [[ "$LINK_TARGET" == "/etc/"* ]]; then
            ((RACE_FAILURES++))
        fi
    fi
    sleep 0.01
done

# Stop the background process
kill $RACE_PID 2>/dev/null || true
wait $RACE_PID 2>/dev/null || true

if [[ $RACE_FAILURES -eq 0 ]]; then
    echo "✅ PASS (No dangerous mounts allowed during race)"
    ((PASSED_TESTS++))
else
    echo "❌ FAIL ($RACE_FAILURES dangerous mounts allowed during race)"
    ((FAILED_TESTS++))
fi

((TOTAL_TESTS++))

# Cleanup race test
cleanup_test_files "$RACE_TEST_DIR"

# Test 9: Performance impact of TOCTOU protection
echo ""
echo "🔍 Performance Impact Test:"
echo "==========================="

PERF_TEST_DIR="/tmp/toctou-perf-test-$$"
mkdir -p "$PERF_TEST_DIR/target"
ln -s "$PERF_TEST_DIR/target" "$PERF_TEST_DIR/link"

# Test without TOCTOU protection (simulate old behavior)
start_time=$(date +%s%N)
for i in {1..50}; do
    validate_mount_security "$PERF_TEST_DIR/link" >/dev/null 2>&1 || true
done
end_time=$(date +%s%N)
old_duration=$(( (end_time - start_time) / 1000000 ))  # Convert to milliseconds

# Test with TOCTOU protection (current behavior)
start_time=$(date +%s%N)
for i in {1..50}; do
    set +e
    process_single_mount "$PERF_TEST_DIR/link" >/dev/null 2>&1
    set -e
done
end_time=$(date +%s%N)
new_duration=$(( (end_time - start_time) / 1000000 ))  # Convert to milliseconds

cleanup_test_files "$PERF_TEST_DIR"

echo "Validation time (50 iterations):"
echo "- Original validation: ${old_duration}ms"
echo "- With TOCTOU protection: ${new_duration}ms"

if [[ $new_duration -gt 0 ]] && [[ $old_duration -gt 0 ]]; then
    overhead=$(( (new_duration * 100 / old_duration) - 100 ))
    echo "- Performance overhead: ${overhead}%"
    
    if [[ $overhead -lt 50 ]]; then
        echo "✅ Performance overhead acceptable (< 50%)"
    else
        echo "⚠️ Performance overhead significant (> 50%)"
    fi
fi

# Test 10: Atomic validation effectiveness
echo ""
echo "🔐 Atomic Validation Test:"
echo "========================="

ATOMIC_TEST_DIR="/tmp/toctou-atomic-test-$$"
mkdir -p "$ATOMIC_TEST_DIR/safe"

# Create a symlink to safe directory
ln -s "$ATOMIC_TEST_DIR/safe" "$ATOMIC_TEST_DIR/atomic-link"

echo -n "Atomic validation consistency test... "

# Test that atomic validation gives consistent results
CONSISTENCY_FAILURES=0
for i in {1..20}; do
    # Get the resolved path
    RESOLVED_PATH=$(readlink -f "$ATOMIC_TEST_DIR/atomic-link" 2>/dev/null)
    
    # Test atomic validation
    set +e
    validate_mount_security_atomic "$RESOLVED_PATH" >/dev/null 2>&1
    local atomic_result=$?
    set -e
    
    if [[ $atomic_result -ne 0 ]]; then
        ((CONSISTENCY_FAILURES++))
    fi
done

if [[ $CONSISTENCY_FAILURES -eq 0 ]]; then
    echo "✅ PASS (Atomic validation consistent)"
    ((PASSED_TESTS++))
else
    echo "❌ FAIL ($CONSISTENCY_FAILURES inconsistent results)"
    ((FAILED_TESTS++))
fi

((TOTAL_TESTS++))

cleanup_test_files "$ATOMIC_TEST_DIR"

# Summary
echo ""
echo "========================================"
echo "Test Results Summary:"
echo "Total tests: $TOTAL_TESTS"
echo "Passed: $PASSED_TESTS"
echo "Failed: $FAILED_TESTS"

if [[ $FAILED_TESTS -eq 0 ]]; then
    echo "🎉 All TOCTOU security tests passed!"
    exit 0
else
    echo "❌ Some TOCTOU security tests failed"
    exit 1
fi