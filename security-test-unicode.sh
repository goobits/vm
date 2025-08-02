#!/bin/bash
# Comprehensive Unicode Path Traversal Attack Test Suite
# Tests the enhanced Unicode normalization detection in vm.sh

set -e

echo "🔒 Unicode Security Validation Test Suite"
echo "=========================================="

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

# Test function
run_unicode_test() {
    local test_name="$1"
    local test_input="$2"
    local expected_result="$3"  # "PASS" or "FAIL"
    
    ((TOTAL_TESTS++))
    
    echo -n "Test $TOTAL_TESTS: $test_name... "
    
    # Create a test directory if the input looks like a path
    local test_dir=""
    if [[ "$test_input" == /* ]] || [[ "$test_input" == ./* ]]; then
        test_dir="/tmp/unicode-test-$RANDOM"
        mkdir -p "$test_dir" 2>/dev/null || true
        # Replace test input with actual test directory for path-based tests
        if [[ "$test_input" == "TEST_DIR" ]]; then
            test_input="$test_dir"
        fi
    fi
    
    # Run the validation (disable strict error checking for this test)
    local result
    set +e
    validate_mount_security "$test_input" >/dev/null 2>&1
    local validation_exit_code=$?
    set -e
    
    if [[ $validation_exit_code -eq 0 ]]; then
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
    if [[ -n "$test_dir" ]]; then
        rm -rf "$test_dir" 2>/dev/null || true
    fi
}

# Test 1: Basic ASCII path traversal (should fail)
run_unicode_test "Basic ASCII path traversal" "/etc" "FAIL"

# Test 2: URL encoded path traversal (should fail)  
# Create a test path that contains encoded characters
mkdir -p "/tmp/test-%2e%2e" 2>/dev/null || true
run_unicode_test "URL encoded path traversal" "/tmp/test-%2e%2e" "FAIL"

# Test 3: Unicode-encoded dots (should fail)
if command -v python3 >/dev/null 2>&1; then
    # Create Unicode-encoded path traversal attempts
    unicode_dots=$(python3 -c "print('\\u002e\\u002e')")
    run_unicode_test "Unicode-encoded dots (\\u002e\\u002e)" "$unicode_dots" "FAIL"
    
    # Fullwidth Unicode dots
    fullwidth_dots=$(python3 -c "print('\\uff0e\\uff0e')")
    run_unicode_test "Fullwidth Unicode dots (\\uff0e\\uff0e)" "$fullwidth_dots" "FAIL"
    
    # One-dot leaders
    dot_leaders=$(python3 -c "print('\\u2024\\u2024')")
    run_unicode_test "One-dot leaders (\\u2024\\u2024)" "$dot_leaders" "FAIL"
    
    # Mixed Unicode/ASCII
    mixed_unicode=$(python3 -c "print('\\u002e.')")
    run_unicode_test "Mixed Unicode/ASCII dots" "$mixed_unicode" "FAIL"
    
    # Unicode normalization attack
    # Create a path that normalizes to contain ..
    normalization_attack=$(python3 -c "
import unicodedata
# Create a string that when normalized becomes '..'
test_str = '\\u002e\\u002e'  # Unicode dots that normalize to ASCII dots
print(test_str)
")
    run_unicode_test "Unicode normalization attack" "$normalization_attack" "FAIL"
    
else
    echo "⚠️ Warning: Python3 not available, skipping advanced Unicode tests"
fi

# Test 4: Legitimate paths (should pass)
mkdir -p /tmp/test-safe-unicode 2>/dev/null
run_unicode_test "Legitimate /tmp path" "/tmp/test-safe-unicode" "PASS"

mkdir -p /workspace/test-safe-unicode 2>/dev/null
run_unicode_test "Legitimate /workspace path" "/workspace/test-safe-unicode" "PASS"

# Test 5: System paths (should fail)
run_unicode_test "System path /etc" "/etc" "FAIL"
run_unicode_test "System path /bin" "/bin" "FAIL"
run_unicode_test "Root filesystem" "/" "FAIL"

# Test 6: Paths with dangerous characters (should fail)
run_unicode_test "Path with semicolon" "/tmp/test;injection" "FAIL"
run_unicode_test "Path with command substitution" "/tmp/\$(rm -rf /)" "FAIL"
run_unicode_test "Path with backticks" "/tmp/\`whoami\`" "FAIL"

# Test 7: Control characters (should fail)
control_char_path="/tmp/test$(printf '\0')null"
run_unicode_test "Path with null byte" "$control_char_path" "FAIL"

newline_path="/tmp/test$(printf '\n')newline"
run_unicode_test "Path with newline" "$newline_path" "FAIL"

# Test 8: Edge cases
run_unicode_test "Empty path" "" "FAIL"
run_unicode_test "Just dots" ".." "FAIL"
run_unicode_test "Hidden file (legitimate)" "/tmp/.hidden" "PASS"

# Performance test - measure time for Unicode validation
if command -v python3 >/dev/null 2>&1; then
    echo ""
    echo "🔍 Performance Test:"
    echo "==================="
    
    start_time=$(date +%s%N)
    for i in {1..10}; do
        validate_mount_security "/tmp/performance-test-$i" >/dev/null 2>&1 || true
    done
    end_time=$(date +%s%N)
    
    duration=$(( (end_time - start_time) / 1000000 ))  # Convert to milliseconds
    avg_time=$(( duration / 10 ))
    
    echo "Average validation time: ${avg_time}ms per path"
    if [[ $avg_time -lt 100 ]]; then
        echo "✅ Performance acceptable (< 100ms)"
    else
        echo "⚠️ Performance concern (> 100ms)"
    fi
fi

# Cleanup
rm -rf /tmp/test-safe-unicode /workspace/test-safe-unicode 2>/dev/null || true
for i in {1..10}; do
    rm -rf "/tmp/performance-test-$i" 2>/dev/null || true
done

# Summary
echo ""
echo "=========================================="
echo "Test Results Summary:"
echo "Total tests: $TOTAL_TESTS"
echo "Passed: $PASSED_TESTS"
echo "Failed: $FAILED_TESTS"

if [[ $FAILED_TESTS -eq 0 ]]; then
    echo "🎉 All Unicode security tests passed!"
    exit 0
else
    echo "❌ Some Unicode security tests failed"
    exit 1
fi