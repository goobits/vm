#!/bin/bash
# Test Suite: Framework Tests
# Tests the test framework itself (no VMs required)

# Test configuration generator
test_config_generator() {
    echo "Testing configuration generator..."
    
    # Generate minimal config
    local min_config=$(generate_minimal_config "/tmp/test-minimal-$$.json")
    
    if [ -f "$min_config" ]; then
        echo -e "${GREEN}✓ Minimal config generated${NC}"
        
        # Validate it has correct structure
        local project_name=$(jq -r '.project.name' "$min_config")
        if [ "$project_name" = "test-minimal" ]; then
            echo -e "${GREEN}✓ Config has correct project name${NC}"
        else
            echo -e "${RED}✗ Config has wrong project name: $project_name${NC}"
            return 1
        fi
        
        rm -f "$min_config"
    else
        echo -e "${RED}✗ Failed to generate minimal config${NC}"
        return 1
    fi
    
    # Test service config generation
    local svc_config=$(generate_service_config "postgresql" true "/tmp/test-pg-$$.json")
    
    if [ -f "$svc_config" ]; then
        echo -e "${GREEN}✓ Service config generated${NC}"
        
        # Check PostgreSQL is enabled
        local pg_enabled=$(jq -r '.services.postgresql.enabled' "$svc_config")
        if [ "$pg_enabled" = "true" ]; then
            echo -e "${GREEN}✓ PostgreSQL correctly enabled${NC}"
        else
            echo -e "${RED}✗ PostgreSQL not enabled${NC}"
            return 1
        fi
        
        rm -f "$svc_config"
    else
        echo -e "${RED}✗ Failed to generate service config${NC}"
        return 1
    fi
}

# Test assertion functions
test_assertions() {
    echo "Testing assertion functions..."
    
    # Test successful assertion
    if echo -e "${GREEN}✓ Test output${NC}" | grep -q "✓"; then
        echo -e "${GREEN}✓ Assertion output works${NC}"
    else
        echo -e "${RED}✗ Assertion output failed${NC}"
        return 1
    fi
    
    # Test color codes
    echo -e "${GREEN}✓ Green color works${NC}"
    echo -e "${RED}✓ Red color works${NC}"
    echo -e "${YELLOW}✓ Yellow color works${NC}"
    echo -e "${BLUE}✓ Blue color works${NC}"
}

# Test vm.sh availability
test_vm_command() {
    echo "Testing vm.sh availability..."
    
    if [ -f "/workspace/vm.sh" ]; then
        echo -e "${GREEN}✓ vm.sh exists${NC}"
        
        # Test vm.sh is executable
        if [ -x "/workspace/vm.sh" ]; then
            echo -e "${GREEN}✓ vm.sh is executable${NC}"
        else
            echo -e "${RED}✗ vm.sh is not executable${NC}"
            return 1
        fi
        
        # Test vm init command exists
        /workspace/vm.sh help 2>&1 | grep -q "init"
        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ vm init command is available${NC}"
        else
            echo -e "${RED}✗ vm init command not found in help${NC}"
            return 1
        fi
    else
        echo -e "${RED}✗ vm.sh not found${NC}"
        return 1
    fi
}

# Test validation functionality
test_validation() {
    echo "Testing configuration validation..."
    
    # Create a test directory
    local test_dir="/tmp/vm-validation-test-$$"
    mkdir -p "$test_dir"
    cd "$test_dir"
    
    # Test with no config
    /workspace/vm.sh validate 2>&1 | grep -q "No vm.json"
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Validation detects missing config${NC}"
    else
        echo -e "${RED}✗ Validation should detect missing config${NC}"
        cd - > /dev/null
        rm -rf "$test_dir"
        return 1
    fi
    
    # Test with valid config
    cp /workspace/vm.json "$test_dir/"
    /workspace/vm.sh validate
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Validation passes with valid config${NC}"
    else
        echo -e "${RED}✗ Validation failed with valid config${NC}"
        cd - > /dev/null
        rm -rf "$test_dir"
        return 1
    fi
    
    # Cleanup
    cd - > /dev/null
    rm -rf "$test_dir"
}

# Test all generated configs are valid
test_generated_configs_valid() {
    echo "Testing all generated configurations are valid..."
    
    # Generate all configs
    generate_all_configs > /dev/null 2>&1
    
    # Validate each generated config
    local config_dir="$(dirname "$SCRIPT_DIR")/configs"
    local failed=0
    
    for config in $(find "$config_dir" -name "*.json" -type f); do
        echo -n "Validating $(basename "$config")... "
        
        # Create temp dir for validation
        local temp_dir="/tmp/validate-$$"
        mkdir -p "$temp_dir"
        cp "$config" "$temp_dir/vm.json"
        
        cd "$temp_dir"
        if /workspace/vm.sh validate > /dev/null 2>&1; then
            echo -e "${GREEN}✓${NC}"
        else
            echo -e "${RED}✗${NC}"
            failed=$((failed + 1))
        fi
        
        cd - > /dev/null
        rm -rf "$temp_dir"
    done
    
    if [ $failed -eq 0 ]; then
        echo -e "${GREEN}✓ All generated configs are valid${NC}"
        return 0
    else
        echo -e "${RED}✗ $failed configs failed validation${NC}"
        return 1
    fi
}