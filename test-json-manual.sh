#!/bin/bash
# Manual test for JSON config rejection

set -e

echo "Creating test directory..."
cd /workspace/test-json-reject

echo "Testing JSON config rejection..."
echo "Running: vm --config config.json status"

# Capture output
output=$(vm --config config.json status 2>&1) || exit_code=$?

echo "Exit code: ${exit_code:-0}"
echo "Output:"
echo "$output"

# Check for rejection message
if echo "$output" | grep -q "JSON configs are no longer supported"; then
    echo "✅ JSON rejection message found"
else
    echo "❌ JSON rejection message NOT found"
fi

# Check for migration suggestion
if echo "$output" | grep -q "vm migrate --input"; then
    echo "✅ Migration suggestion found"
else
    echo "❌ Migration suggestion NOT found"
fi