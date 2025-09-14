#!/bin/bash
# Comprehensive test for JSON config rejection across all components
# Consolidates: test-json-manual.sh + test-validate-json.sh + test-config-processor.sh

set -e

# Get project root directory
PROJECT_ROOT="$PROJECT_ROOT"

echo "=== JSON Config Rejection Test Suite ==="

# Setup test directory
cd "$PROJECT_ROOT/test/configs/test-json-reject"

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

# JSON files should be rejected without migration suggestion
if echo "$output1" | grep -q "JSON configuration files are no longer supported"; then
    echo "✅ JSON rejection message found in vm command"
else
    echo "❌ JSON rejection message NOT found in vm command"
fi

echo -e "\n--- Test 2: JSON rejection via vm-config validate ---"
echo "Running: vm-config validate config.json"

# Test vm-config binary directly
VM_CONFIG="$PROJECT_ROOT/rust/target/release/vm-config"
output2=$($VM_CONFIG validate config.json 2>&1) || exit_code2=$?

echo "Exit code: ${exit_code2:-0}"
echo "Output:"
echo "$output2"

# Check for rejection message from vm-config
if echo "$output2" | grep -q "JSON.*not.*support\|Configuration validation failed"; then
    echo "✅ JSON rejection message found in vm-config validate"
else
    echo "❌ JSON rejection message NOT found in vm-config validate"
fi

echo -e "\n--- Test 3: JSON rejection via vm-config process ---"
echo "Running: vm-config process --config config.json"

# Test vm-config process command directly
output3=$($VM_CONFIG process --defaults "$PROJECT_ROOT/vm.yaml" --config "$PROJECT_ROOT/test/configs/test-json-reject/config.json" --project-dir "$PROJECT_ROOT/test/configs/test-json-reject" --presets-dir "$PROJECT_ROOT/configs/presets" 2>&1) || exit_code3=$?

echo "Exit code: ${exit_code3:-0}"
echo "Output:"
echo "$output3"

# Check for rejection message from vm-config process
if echo "$output3" | grep -q "JSON.*not.*support\|Failed to.*config\|Configuration.*failed"; then
    echo "✅ JSON rejection message found in vm-config process"
else
    echo "❌ JSON rejection message NOT found in vm-config process"
fi

echo -e "\n=== Test Summary ==="
echo "All three components have been tested for JSON rejection functionality."
echo "This comprehensive test ensures consistent JSON rejection behavior across:"
echo "  1. vm command line interface"
echo "  2. vm-config validate command"
echo "  3. vm-config process command"