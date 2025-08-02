#!/bin/bash

echo "🔒 Final Security Validation"
echo "============================"

# Test 1: Check that dangerous characters are rejected
echo "Test 1: Dangerous character protection"
result1=$(bash -c 'source vm.sh >/dev/null 2>&1; validate_mount_security "path;with;semicolons" 2>/dev/null' && echo "FAIL" || echo "PASS")
echo "  Semicolons: $result1"

result2=$(bash -c 'source vm.sh >/dev/null 2>&1; validate_mount_security "path\"with\"quotes" 2>/dev/null' && echo "FAIL" || echo "PASS")
echo "  Quotes: $result2"

result3=$(bash -c 'source vm.sh >/dev/null 2>&1; validate_mount_security "$(rm -rf /)" 2>/dev/null' && echo "FAIL" || echo "PASS")
echo "  Command injection: $result3"

# Test 2: Check that system paths are rejected
echo ""
echo "Test 2: System path protection"
result4=$(bash -c 'source vm.sh >/dev/null 2>&1; validate_mount_security "/etc" 2>/dev/null' && echo "FAIL" || echo "PASS")
echo "  /etc protection: $result4"

result5=$(bash -c 'source vm.sh >/dev/null 2>&1; validate_mount_security "/bin" 2>/dev/null' && echo "FAIL" || echo "PASS")
echo "  /bin protection: $result5"

# Test 3: Check that safe paths are allowed
echo ""
echo "Test 3: Safe path allowance"
mkdir -p /tmp/safe-test 2>/dev/null
result6=$(bash -c 'source vm.sh >/dev/null 2>&1; validate_mount_security "/tmp/safe-test" 2>/dev/null' && echo "PASS" || echo "FAIL")
echo "  /tmp paths: $result6"

mkdir -p /workspace/safe-test 2>/dev/null
result7=$(bash -c 'source vm.sh >/dev/null 2>&1; validate_mount_security "/workspace/safe-test" 2>/dev/null' && echo "PASS" || echo "FAIL")
echo "  /workspace paths: $result7"

# Test 4: Temp file utilities
echo ""
echo "Test 4: Temp file security"
if bash -c 'source vm.sh >/dev/null 2>&1; type create_secure_temp_file >/dev/null 2>&1'; then
    echo "  Temp file utils: PASS"
else
    echo "  Temp file utils: FAIL"
fi

# Cleanup
rm -rf /tmp/safe-test /workspace/safe-test 2>/dev/null

# Summary
echo ""
echo "============================"
total_pass=0
for result in "$result1" "$result2" "$result3" "$result4" "$result5" "$result6" "$result7"; do
    if [[ "$result" == "PASS" ]]; then
        ((total_pass++))
    fi
done

echo "✅ Security tests: $total_pass/7 passed"
if [[ $total_pass -eq 7 ]]; then
    echo "🎉 All security measures working correctly!"
else
    echo "⚠️  Some security measures need attention"
fi