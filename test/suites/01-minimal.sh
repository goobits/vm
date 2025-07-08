#!/bin/bash
# Test Suite: Minimal Configuration
# Tests that the VM works with the absolute minimum configuration

# Test that VM boots with minimal config
test_minimal_boot() {
    echo "Testing VM boot with minimal configuration..."
    
    # Create VM with minimal config
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Basic checks
    assert_vm_running
    assert_command_succeeds "whoami" "User check"
    assert_output_contains "pwd" "/workspace" "Working directory check"
}

# Test basic functionality without services
test_minimal_functionality() {
    echo "Testing basic functionality..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Check basic commands work
    assert_command_succeeds "ls -la" "List files"
    assert_command_succeeds "cd /tmp && pwd" "Change directory"
    assert_command_succeeds "echo 'test' > /tmp/testfile" "Write file"
    assert_command_succeeds "cat /tmp/testfile" "Read file"
    
    # Check workspace is mounted
    assert_command_succeeds "ls /workspace" "Workspace mounted"
    assert_file_exists "/workspace/vm.sh" "VM tool available in workspace"
}

# Test that no services are installed
test_no_services_installed() {
    echo "Testing that no services are installed..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Check services are NOT installed
    assert_service_not_enabled "postgresql" "PostgreSQL should not be installed"
    assert_service_not_enabled "redis" "Redis should not be installed"
    assert_service_not_enabled "mongodb" "MongoDB should not be installed"
    assert_service_not_enabled "docker" "Docker should not be installed"
    
    # Check no extra packages
    assert_command_fails "which prettier" "Prettier should not be installed"
    assert_command_fails "which eslint" "ESLint should not be installed"
    assert_command_fails "which cargo" "Rust should not be installed"
}

# Test minimal shell environment
test_minimal_shell_environment() {
    echo "Testing minimal shell environment..."
    
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Check shell is zsh
    assert_output_contains "echo \$SHELL" "/usr/bin/zsh" "Shell should be zsh"
    
    # Check basic environment
    assert_command_succeeds "source ~/.zshrc" "Zshrc loads successfully"
    
    # Check NVM is installed (it's part of base setup)
    assert_command_succeeds "source ~/.zshrc && nvm --version" "NVM is available"
    
    # Check Node.js is installed
    assert_command_succeeds "source ~/.zshrc && node --version" "Node.js is available"
    
    # Check terminal customization
    assert_output_contains "echo \$USER" "vagrant" "Default user"
}

# Test minimal config with custom project name
test_minimal_custom_project() {
    echo "Testing minimal config with custom project name..."
    
    # Generate custom minimal config
    local custom_config="/tmp/minimal-custom-$$.json"
    generate_config "minimal-custom" '{
        "project": {
            "name": "custom-project",
            "hostname": "dev.custom.local"
        },
        "terminal": {
            "emoji": "🔧",
            "username": "custom-dev"
        }
    }' "$custom_config"
    
    create_test_vm "$custom_config" || return 1
    
    # Check customizations applied
    assert_output_contains "hostname" "dev.custom.local" "Custom hostname set"
    
    # Cleanup custom config
    rm -f "$custom_config"
}

# Test that minimal config can be extended
test_minimal_extensibility() {
    echo "Testing that minimal config can be extended..."
    
    # Start with minimal config
    create_test_vm "$CONFIG_DIR/minimal.json" || return 1
    
    # Verify we can still install things manually
    assert_command_succeeds "sudo apt-get update" "Can run apt update"
    assert_command_succeeds "pip3 --version" "Python pip is available"
    
    # Test that we can create files in workspace
    local test_file="/workspace/test-minimal-$$.txt"
    assert_command_succeeds "echo 'test' > $test_file" "Can write to workspace"
    
    # Verify file exists on host
    if [ -f "$test_file" ]; then
        echo -e "${GREEN}✓ File synced to host${NC}"
        rm -f "$test_file"
    else
        echo -e "${RED}✗ File not synced to host${NC}"
        return 1
    fi
}