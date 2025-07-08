#!/bin/bash
# Test Suite: CLI Command Tests
# Tests the vm.sh wrapper commands

# Test vm init command
test_vm_init() {
    echo "Testing vm init command..."
    
    # Setup test directory
    local init_dir="$TEST_DIR/init-test"
    mkdir -p "$init_dir"
    cd "$init_dir"
    
    # Run vm init
    vm init
    local exit_code=$?
    
    # Check exit code
    if [ $exit_code -ne 0 ]; then
        echo -e "${RED}✗ vm init failed with exit code $exit_code${NC}"
        return 1
    fi
    
    # Check vm.json was created
    if [ ! -f "vm.json" ]; then
        echo -e "${RED}✗ vm.json was not created${NC}"
        return 1
    fi
    
    # Check content is customized
    local project_name=$(jq -r '.project.name' vm.json)
    if [ "$project_name" != "init-test" ]; then
        echo -e "${RED}✗ Project name not customized (got: $project_name)${NC}"
        return 1
    fi
    
    echo -e "${GREEN}✓ vm init creates customized config${NC}"
    
    # Test init with existing file
    vm init 2>&1 | grep -q "already exists"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm init prevents overwriting existing config${NC}"
    else
        echo -e "${RED}✗ vm init should prevent overwriting${NC}"
        return 1
    fi
}

# Test vm validate command
test_vm_validate() {
    echo "Testing vm validate command..."
    
    # Test with valid config
    cd "$TEST_DIR"
    cp "$CONFIG_DIR/minimal.json" vm.json
    
    vm validate
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm validate succeeds with valid config${NC}"
    else
        echo -e "${RED}✗ vm validate failed with valid config${NC}"
        return 1
    fi
    
    # Test with invalid config
    echo '{"invalid": "config"}' > vm.json
    vm validate 2>&1 | grep -q -E "(error|invalid|failed)"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm validate detects invalid config${NC}"
    else
        echo -e "${RED}✗ vm validate should detect invalid config${NC}"
        return 1
    fi
    
    # Test with missing config
    rm -f vm.json
    vm validate 2>&1 | grep -q "No vm.json"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm validate reports missing config${NC}"
    else
        echo -e "${RED}✗ vm validate should report missing config${NC}"
        return 1
    fi
}

# Test vm status command
test_vm_status() {
    echo "Testing vm status command..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Check status when running
    cd "$TEST_DIR"
    local status_output=$(vm status 2>&1)
    
    if echo "$status_output" | grep -q "running"; then
        echo -e "${GREEN}✓ vm status shows running state${NC}"
    else
        echo -e "${RED}✗ vm status should show running state${NC}"
        echo "Output: $status_output"
        return 1
    fi
    
    # Halt VM
    vm halt || return 1
    sleep 5
    
    # Check status when stopped
    status_output=$(vm status 2>&1)
    if echo "$status_output" | grep -q -E "(stopped|poweroff|halted)"; then
        echo -e "${GREEN}✓ vm status shows stopped state${NC}"
    else
        echo -e "${RED}✗ vm status should show stopped state${NC}"
        echo "Output: $status_output"
        return 1
    fi
}

# Test vm list command
test_vm_list() {
    echo "Testing vm list command..."
    
    # Create a VM first
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Run vm list
    local list_output=$(vm list 2>&1)
    
    # Check output contains VM info
    if echo "$list_output" | grep -q -E "(Docker VMs|test-minimal)"; then
        echo -e "${GREEN}✓ vm list shows running VMs${NC}"
    else
        echo -e "${RED}✗ vm list should show running VMs${NC}"
        echo "Output: $list_output"
        return 1
    fi
}

# Test vm exec command
test_vm_exec() {
    echo "Testing vm exec command..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Test simple command
    local output=$(vm exec "echo hello" 2>&1)
    if echo "$output" | grep -q "hello"; then
        echo -e "${GREEN}✓ vm exec runs commands${NC}"
    else
        echo -e "${RED}✗ vm exec should run commands${NC}"
        echo "Output: $output"
        return 1
    fi
    
    # Test command with exit code
    vm exec "exit 0"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm exec preserves exit codes${NC}"
    else
        echo -e "${RED}✗ vm exec should preserve exit codes${NC}"
        return 1
    fi
    
    # Test failing command
    vm exec "exit 42"
    if [ $? -eq 42 ]; then
        echo -e "${GREEN}✓ vm exec preserves error codes${NC}"
    else
        echo -e "${RED}✗ vm exec should preserve error exit codes${NC}"
        return 1
    fi
}

# Test vm ssh command
test_vm_ssh() {
    echo "Testing vm ssh command..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Can't test interactive SSH easily, but test that command exists
    vm ssh --help 2>&1 | grep -q -E "(ssh|connect)"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm ssh command exists${NC}"
    else
        # Try without --help
        vm ssh "exit 0" 2>&1
        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ vm ssh command works${NC}"
        else
            echo -e "${YELLOW}⚠ vm ssh command may require TTY${NC}"
        fi
    fi
}

# Test vm reload command
test_vm_reload() {
    echo "Testing vm reload command..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Create a test file in VM
    vm exec "echo 'before reload' > /tmp/reload-test"
    
    # Modify config (add an alias)
    jq '.aliases.testreload = "echo reload-success"' vm.json > vm.json.tmp
    mv vm.json.tmp vm.json
    
    # Reload VM
    vm reload || return 1
    sleep 10  # Give time for provisioning
    
    # Check file persists
    local file_content=$(vm exec "cat /tmp/reload-test 2>/dev/null" || echo "")
    if echo "$file_content" | grep -q "before reload"; then
        echo -e "${GREEN}✓ vm reload preserves VM state${NC}"
    else
        echo -e "${YELLOW}⚠ vm reload may have reset VM state${NC}"
    fi
    
    # Check new alias is available
    vm exec "source ~/.zshrc && type testreload" 2>&1 | grep -q "alias"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm reload applies config changes${NC}"
    else
        echo -e "${RED}✗ vm reload should apply config changes${NC}"
        return 1
    fi
}

# Test vm destroy command
test_vm_destroy() {
    echo "Testing vm destroy command..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    cd "$TEST_DIR"
    
    # Destroy VM
    vm destroy -f || return 1
    
    # Check VM is gone
    vm status 2>&1 | grep -q -E "(not created|not found|no such)"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ vm destroy removes VM${NC}"
    else
        echo -e "${RED}✗ vm destroy should remove VM${NC}"
        return 1
    fi
    
    # Check we can create it again
    vm up || return 1
    assert_vm_running
    echo -e "${GREEN}✓ Can recreate VM after destroy${NC}"
}