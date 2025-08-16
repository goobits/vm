#!/bin/bash
# Test validate-config.sh with JSON file

set -e

echo "Testing validate-config.sh with JSON file..."
cd /workspace/test-json-reject

echo "Running: validate-config.sh --get-config config.json"

# Test validate-config.sh directly
output=$(/workspace/validate-config.sh --get-config config.json 2>&1) || exit_code=$?

echo "Exit code: ${exit_code:-0}"
echo "Output:"
echo "$output"

# Check for rejection message
if echo "$output" | grep -q "JSON configs are no longer supported"; then
    echo "✅ JSON rejection message found in validate-config.sh"
else
    echo "❌ JSON rejection message NOT found in validate-config.sh"
fi