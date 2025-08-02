#!/bin/bash

# Simple security test script for VM improvements
echo "🔒 Testing Security Improvements"
echo "================================="

# Source the vm.sh to get the real validation function
source /workspace/vm.sh

# Test 1: Mount validation should reject dangerous paths
echo "Test 1: Mount validation security"

dangerous_paths=(
    "/etc"
    "/bin" 
    "../../../etc/passwd"
    "path;with;semicolons"
    'path"with"quotes'
    '$(rm -rf /)'
)

for test_path in "${dangerous_paths[@]}"; do
    if validate_mount_security "$test_path" 2>/dev/null; then
        echo "❌ FAIL: Dangerous path was allowed: $test_path"
    else
        echo "✅ PASS: Correctly rejected: $test_path"
    fi
done

# Test 2: Mount validation should allow safe paths
echo ""
echo "Test 2: Safe paths should be allowed"

safe_paths=(
    "/home/user/project"
    "/workspace/src"
    "/tmp/safe"
)

for safe_path in "${safe_paths[@]}"; do
    # Create the path temporarily for testing
    mkdir -p "$safe_path" 2>/dev/null
    
    if validate_mount_security "$safe_path" 2>/dev/null; then
        echo "✅ PASS: Correctly allowed: $safe_path"
    else
        echo "❌ FAIL: Safe path was rejected: $safe_path"
    fi
    
    # Clean up
    rmdir "$safe_path" 2>/dev/null || true
done

# Test 3: Temp file tracking
echo ""
echo "Test 3: Temp file creation and cleanup"

# Test temp file creation
temp_file=$(create_secure_temp_file "security-test")
if [[ -f "$temp_file" ]]; then
    echo "✅ PASS: Temp file created successfully: $temp_file"
    
    # Check if tracking is working
    temp_count=$(list_tracked_temp_files | wc -l)
    if [[ $temp_count -gt 0 ]]; then
        echo "✅ PASS: Temp file tracking working (count: $temp_count)"
    else
        echo "❌ FAIL: Temp file tracking not working"
    fi
else
    echo "❌ FAIL: Temp file creation failed"
fi

# Test 4: Docker command wrapper
echo ""
echo "Test 4: Docker command wrapper"

if command -v docker_cmd >/dev/null 2>&1; then
    echo "✅ PASS: Docker command wrapper working"
else
    echo "❌ FAIL: Docker command wrapper not found"
fi

# Summary
echo ""
echo "================================="
echo "✅ Security tests completed!"
echo "⚠️  Review any failures above"