#!/bin/bash
# VM Provider Validation Test Suite
# Tests both Vagrant and Docker providers end-to-end

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test results
VAGRANT_PASSED=0
DOCKER_PASSED=0

echo "🧪 VM Provider Validation Test Suite"
echo "===================================="
echo ""

# Function to run a test command
run_test() {
    local provider="$1"
    local command="$2"
    local expected="$3"
    local description="$4"
    
    echo -n "  Testing $description... "
    
    # Run command and capture output
    if output=$(./vm.sh exec "$command" 2>&1); then
        if [ -n "$expected" ]; then
            if echo "$output" | grep -q "$expected"; then
                echo -e "${GREEN}✓${NC}"
                return 0
            else
                echo -e "${RED}✗${NC} (expected '$expected', got '$output')"
                return 1
            fi
        else
            echo -e "${GREEN}✓${NC}"
            return 0
        fi
    else
        echo -e "${RED}✗${NC} (command failed)"
        return 1
    fi
}

# Function to test a provider
test_provider() {
    local provider="$1"
    local config_file="$2"
    local passed=0
    local total=0
    
    echo ""
    echo "Testing $provider provider with $config_file"
    echo "----------------------------------------"
    
    # Start the VM
    echo "1. Starting VM..."
    if ./vm.sh -c "$config_file" up > /dev/null 2>&1; then
        echo -e "   ${GREEN}✓${NC} VM started successfully"
        
        # Wait for VM to be ready
        echo "2. Waiting for VM to be ready..."
        sleep 10
        
        # Run tests
        echo "3. Running validation tests:"
        
        # Test Node.js installation
        total=$((total + 1))
        if run_test "$provider" "node --version" "v" "Node.js installation"; then
            passed=$((passed + 1))
        fi
        
        # Test pnpm installation
        total=$((total + 1))
        if run_test "$provider" "pnpm --version" "" "pnpm installation"; then
            passed=$((passed + 1))
        fi
        
        # Test Git installation
        total=$((total + 1))
        if run_test "$provider" "git --version" "git version" "Git installation"; then
            passed=$((passed + 1))
        fi
        
        # Test PostgreSQL if enabled
        if grep -q '"postgresql".*"enabled": true' "$config_file" 2>/dev/null; then
            total=$((total + 1))
            if run_test "$provider" "psql --version" "psql (PostgreSQL)" "PostgreSQL installation"; then
                passed=$((passed + 1))
            fi
            
            # Test PostgreSQL connection
            total=$((total + 1))
            if run_test "$provider" "PGPASSWORD=postgres psql -h localhost -U postgres -c 'SELECT version();'" "PostgreSQL" "PostgreSQL connection"; then
                passed=$((passed + 1))
            fi
        fi
        
        # Test Redis if enabled
        if grep -q '"redis".*"enabled": true' "$config_file" 2>/dev/null; then
            total=$((total + 1))
            if run_test "$provider" "redis-cli --version" "redis-cli" "Redis installation"; then
                passed=$((passed + 1))
            fi
            
            # Test Redis connection
            total=$((total + 1))
            if run_test "$provider" "redis-cli ping" "PONG" "Redis connection"; then
                passed=$((passed + 1))
            fi
        fi
        
        # Test MongoDB if enabled
        if grep -q '"mongodb".*"enabled": true' "$config_file" 2>/dev/null; then
            total=$((total + 1))
            if run_test "$provider" "mongosh --version" "" "MongoDB shell installation"; then
                passed=$((passed + 1))
            fi
        fi
        
        # Test zsh shell
        total=$((total + 1))
        if run_test "$provider" "echo \$SHELL" "/bin/zsh" "Zsh shell"; then
            passed=$((passed + 1))
        fi
        
        # Test locale settings
        total=$((total + 1))
        if run_test "$provider" "echo \$LANG" "en_US.UTF-8" "Locale configuration"; then
            passed=$((passed + 1))
        fi
        
        # Test workspace directory
        total=$((total + 1))
        if run_test "$provider" "pwd" "/workspace" "Workspace directory"; then
            passed=$((passed + 1))
        fi
        
        # Test Claude settings
        total=$((total + 1))
        if run_test "$provider" "test -f ~/.claude/settings.json && echo exists" "exists" "Claude settings"; then
            passed=$((passed + 1))
        fi
        
        # Clean up
        echo ""
        echo "4. Destroying VM..."
        if ./vm.sh -c "$config_file" destroy -f > /dev/null 2>&1; then
            echo -e "   ${GREEN}✓${NC} VM destroyed successfully"
        else
            echo -e "   ${RED}✗${NC} Failed to destroy VM"
        fi
        
    else
        echo -e "   ${RED}✗${NC} Failed to start VM"
        return 1
    fi
    
    # Summary for this provider
    echo ""
    echo "Results: $passed/$total tests passed"
    
    if [ "$passed" -eq "$total" ]; then
        echo -e "${GREEN}✓ All tests passed for $provider!${NC}"
        return 0
    else
        echo -e "${RED}✗ Some tests failed for $provider${NC}"
        return 1
    fi
}

# Create test configuration files
echo "Creating test configuration files..."

# Vagrant test config
cat > vagrant-vm.json << 'EOF'
{
  "provider": "vagrant",
  "project": {
    "name": "test-vagrant",
    "hostname": "test-vagrant.local",
    "workspace_path": "/workspace"
  },
  "services": {
    "postgresql": {
      "enabled": true,
      "password": "postgres"
    },
    "redis": {
      "enabled": true
    }
  }
}
EOF

# Docker test config
cat > docker-vm.json << 'EOF'
{
  "provider": "docker",
  "project": {
    "name": "test-docker",
    "hostname": "test-docker.local",
    "workspace_path": "/workspace"
  },
  "services": {
    "postgresql": {
      "enabled": true,
      "password": "postgres"
    },
    "redis": {
      "enabled": true
    }
  }
}
EOF

# Test Vagrant provider
if [ -x "$(command -v vagrant)" ]; then
    if test_provider "Vagrant" "vagrant-vm.json"; then
        VAGRANT_PASSED=1
    fi
else
    echo ""
    echo -e "${YELLOW}⚠ Skipping Vagrant tests - Vagrant not installed${NC}"
fi

# Test Docker provider
if [ -x "$(command -v docker)" ]; then
    if test_provider "Docker" "docker-vm.json"; then
        DOCKER_PASSED=1
    fi
else
    echo ""
    echo -e "${YELLOW}⚠ Skipping Docker tests - Docker not installed${NC}"
fi

# Clean up test files
rm -f vagrant-vm.json docker-vm.json

# Final summary
echo ""
echo "===================================="
echo "Final Test Summary"
echo "===================================="

if [ -x "$(command -v vagrant)" ]; then
    if [ "$VAGRANT_PASSED" -eq 1 ]; then
        echo -e "Vagrant Provider: ${GREEN}PASSED${NC}"
    else
        echo -e "Vagrant Provider: ${RED}FAILED${NC}"
    fi
else
    echo -e "Vagrant Provider: ${YELLOW}SKIPPED${NC} (not installed)"
fi

if [ -x "$(command -v docker)" ]; then
    if [ "$DOCKER_PASSED" -eq 1 ]; then
        echo -e "Docker Provider:  ${GREEN}PASSED${NC}"
    else
        echo -e "Docker Provider:  ${RED}FAILED${NC}"
    fi
else
    echo -e "Docker Provider:  ${YELLOW}SKIPPED${NC} (not installed)"
fi

echo ""

# Exit with appropriate code
if [ "$VAGRANT_PASSED" -eq 1 ] && [ "$DOCKER_PASSED" -eq 1 ]; then
    echo -e "${GREEN}✓ All provider tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some provider tests failed${NC}"
    exit 1
fi