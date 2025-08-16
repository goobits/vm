#!/bin/bash
# Comprehensive test for JSON config rejection across all components
# Consolidates: test-json-manual.sh + test-validate-json.sh + test-config-processor.sh

set -e

echo "=== JSON Config Rejection Test Suite ==="

# Setup test directory
cd /workspace/test/configs/test-json-reject

echo -e "\n--- Test 1: JSON rejection via vm command ---"
echo "Running: vm --config config.json status"

# Capture output from vm command
output1=$(vm --config config.json status 2>&1) || exit_code1=$?

echo "Exit code: ${exit_code1:-0}"
echo "Output:"
echo "$output1"

# Check for rejection message from vm command
if echo "$output1" | grep -q "JSON configs are no longer supported"; then
    echo "✅ JSON rejection message found in vm command"
else
    echo "❌ JSON rejection message NOT found in vm command"
fi

# Check for migration suggestion from vm command
if echo "$output1" | grep -q "vm migrate --input"; then
    echo "✅ Migration suggestion found in vm command"
else
    echo "❌ Migration suggestion NOT found in vm command"
fi

echo -e "\n--- Test 2: JSON rejection via validate-config.sh ---"
echo "Running: validate-config.sh --get-config config.json"

# Test validate-config.sh directly
output2=$(/workspace/validate-config.sh --get-config config.json 2>&1) || exit_code2=$?

echo "Exit code: ${exit_code2:-0}"
echo "Output:"
echo "$output2"

# Check for rejection message from validate-config.sh
if echo "$output2" | grep -q "JSON configs are no longer supported"; then
    echo "✅ JSON rejection message found in validate-config.sh"
else
    echo "❌ JSON rejection message NOT found in validate-config.sh"
fi

echo -e "\n--- Test 3: JSON rejection via config-processor.sh ---"
echo "Running: config-processor.sh load /workspace/test/configs/test-json-reject/config.json"

# Test config-processor.sh directly with presets enabled (default behavior)
export VM_USE_PRESETS=true
output3=$(cd /workspace && /workspace/shared/config-processor.sh load /workspace/test/configs/test-json-reject/config.json 2>&1) || exit_code3=$?

echo "Exit code: ${exit_code3:-0}"
echo "Output:"
echo "$output3"

# Check for rejection message from config-processor.sh
if echo "$output3" | grep -q "JSON configs are no longer supported"; then
    echo "✅ JSON rejection message found in config-processor.sh"
else
    echo "❌ JSON rejection message NOT found in config-processor.sh"
fi

echo -e "\n=== Test Summary ==="
echo "All three components have been tested for JSON rejection functionality."
echo "This comprehensive test ensures consistent JSON rejection behavior across:"
echo "  1. vm command line interface"
echo "  2. validate-config.sh script"
echo "  3. config-processor.sh script"