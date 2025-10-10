#!/bin/bash
set -euo pipefail

export CI=true
export VM_NO_PROMPT=true

# Ensure VM is installed
vm --version

# Create test project
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"
echo '{"name": "test"}' > package.json

echo "Testing non-interactive commands..."

# Test 1: Create without prompts
vm create --force || { echo "FAIL: create"; exit 1; }

# Test 2: Status (should not prompt)
vm status || { echo "FAIL: status"; exit 1; }

# Test 3: List (should not prompt)
vm list || { echo "FAIL: list"; exit 1; }

# Test 4: Exec (should not prompt)
vm exec "echo 'test'" || { echo "FAIL: exec"; exit 1; }

# Test 5: SSH with command (should not block)
timeout 10 vm ssh --command "echo 'test'" || { echo "FAIL: ssh"; exit 1; }

# Test 6: Destroy without confirmation
vm destroy --force || { echo "FAIL: destroy"; exit 1; }

echo "All non-interactive tests passed"
