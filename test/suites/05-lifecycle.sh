#!/bin/bash
# Test Suite: VM Lifecycle Tests
# Tests VM state transitions and persistence

# Test halt and resume preserves state
test_halt_resume_state() {
    echo "Testing halt and resume preserves VM state..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Create test data
    vm exec "echo 'test data' > /tmp/persist-test"
    vm exec "mkdir -p /tmp/test-dir && echo 'dir data' > /tmp/test-dir/file.txt"
    
    # Store some data in user directory
    vm exec "echo 'user data' > ~/user-test.txt"
    
    # Halt VM
    vm halt || return 1
    sleep 5
    assert_vm_stopped
    
    # Resume VM
    vm up || return 1
    sleep 5
    assert_vm_running
    
    # Check data persists
    assert_command_succeeds "cat /tmp/persist-test" "Temp file persists"
    assert_output_contains "cat /tmp/persist-test" "test data" "Temp file content correct"
    
    assert_command_succeeds "cat ~/user-test.txt" "User file persists"
    assert_output_contains "cat ~/user-test.txt" "user data" "User file content correct"
}

# Test configuration changes with reload
test_config_reload() {
    echo "Testing configuration changes with reload..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Add an alias
    jq '.aliases.hello = "echo Hello from alias"' vm.json > vm.json.tmp
    mv vm.json.tmp vm.json
    
    # Reload
    vm reload || return 1
    sleep 10
    
    # Test alias works
    assert_command_succeeds "source ~/.zshrc && hello" "New alias available after reload"
    assert_output_contains "source ~/.zshrc && hello" "Hello from alias" "Alias works correctly"
    
    # Add an environment variable
    jq '.environment.TEST_RELOAD = "reload_success"' vm.json > vm.json.tmp
    mv vm.json.tmp vm.json
    
    # Reload again
    vm reload || return 1
    sleep 10
    
    # Test environment variable
    assert_output_contains "source ~/.zshrc && echo \$TEST_RELOAD" "reload_success" "Environment variable set"
}

# Test adding services via reload
test_add_service_reload() {
    echo "Testing adding services via reload..."
    
    # Start with minimal config
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Verify Redis is not installed
    assert_service_not_enabled "redis" "Redis not initially installed"
    
    # Enable Redis
    jq '.services.redis = {enabled: true}' vm.json > vm.json.tmp
    mv vm.json.tmp vm.json
    
    # Reload (this will reprovision)
    vm reload || return 1
    sleep 30  # Services take time to install
    
    # Check Redis is now available
    assert_service_enabled "redis" "Redis installed after reload"
    assert_command_succeeds "redis-cli ping" "Redis is functional"
}

# Test workspace sync across lifecycle
test_workspace_sync() {
    echo "Testing workspace synchronization across lifecycle..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Create file from host
    echo "host file" > host-test.txt
    
    # Check it's visible in VM
    assert_file_exists "/workspace/host-test.txt" "Host file visible in VM"
    assert_output_contains "cat /workspace/host-test.txt" "host file" "Host file content correct"
    
    # Create file from VM
    vm exec "echo 'vm file' > /workspace/vm-test.txt"
    
    # Check it's visible on host
    if [ -f "vm-test.txt" ]; then
        echo -e "${GREEN}✓ VM file visible on host${NC}"
        if grep -q "vm file" vm-test.txt; then
            echo -e "${GREEN}✓ VM file content correct${NC}"
        else
            echo -e "${RED}✗ VM file content incorrect${NC}"
            return 1
        fi
    else
        echo -e "${RED}✗ VM file not visible on host${NC}"
        return 1
    fi
    
    # Test sync after halt/resume
    vm halt || return 1
    sleep 5
    
    # Modify file on host while VM is down
    echo "modified on host" >> host-test.txt
    
    vm up || return 1
    sleep 5
    
    # Check modification is visible in VM
    assert_output_contains "cat /workspace/host-test.txt" "modified on host" "File modifications sync after resume"
}

# Test rapid lifecycle transitions
test_rapid_transitions() {
    echo "Testing rapid lifecycle transitions..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Rapid halt/up cycles
    for i in {1..3}; do
        echo "Cycle $i..."
        
        # Create marker file
        vm exec "echo 'cycle $i' > /tmp/cycle-$i.txt"
        
        # Halt
        vm halt || return 1
        sleep 3
        
        # Resume
        vm up || return 1
        sleep 5
        
        # Verify
        assert_file_exists "/tmp/cycle-$i.txt" "Cycle $i file exists"
    done
    
    echo -e "${GREEN}✓ VM survives rapid transitions${NC}"
}

# Test destroy and recreate
test_destroy_recreate() {
    echo "Testing destroy and recreate..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Create some state
    vm exec "echo 'original' > /tmp/destroy-test.txt"
    
    # Destroy
    vm destroy -f || return 1
    sleep 3
    
    # Verify it's gone
    assert_vm_stopped
    
    # Recreate
    vm up || return 1
    sleep 10
    
    # Check state is fresh (file should not exist)
    assert_file_not_exists "/tmp/destroy-test.txt" "State cleared after destroy"
    
    # But workspace should still be mounted
    assert_file_exists "/workspace/vm.json" "Workspace still mounted after recreate"
}

# Test long-running process survival
test_process_survival() {
    echo "Testing long-running process survival..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Start a background process
    vm exec "nohup sleep 300 > /tmp/sleep.log 2>&1 &"
    vm exec "echo \$! > /tmp/sleep.pid"
    
    # Get PID
    local pid=$(vm exec "cat /tmp/sleep.pid")
    
    # Check process is running
    assert_command_succeeds "ps -p $pid" "Background process running"
    
    # Reload VM
    vm reload || return 1
    sleep 10
    
    # Check if process survived (it probably won't, but let's see)
    vm exec "ps -p $pid" 2>/dev/null
    if [ $? -eq 0 ]; then
        echo -e "${YELLOW}⚠ Background process survived reload (unexpected)${NC}"
    else
        echo -e "${GREEN}✓ Background process terminated on reload (expected)${NC}"
    fi
}