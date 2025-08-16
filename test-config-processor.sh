#!/bin/bash
# Test config-processor.sh with JSON file

set -e

echo "Testing config-processor.sh with JSON file..."
cd /workspace/test-json-reject

echo "Running: config-processor.sh load config.json"

# Test config-processor.sh directly
output=$(cd /workspace && /workspace/shared/config-processor.sh load config.json 2>&1) || exit_code=$?

echo "Exit code: ${exit_code:-0}"
echo "Output:"
echo "$output"

# Check for rejection message
if echo "$output" | grep -q "JSON configs are no longer supported"; then
    echo "✅ JSON rejection message found in config-processor.sh"
else
    echo "❌ JSON rejection message NOT found in config-processor.sh"
fi