#!/bin/bash
# Quick test script for already-running environments
# Usage: ./test-running-env.sh [provider]

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test counters
PASSED=0
FAILED=0

# Function to run a test
test_command() {
    local description="$1"
    local command="$2"
    local expected="$3"
    
    echo -n "  Testing $description... "
    
    if output=$(./vm.sh exec "$command" 2>&1); then
        if [ -n "$expected" ]; then
            if echo "$output" | grep -q "$expected"; then
                echo -e "${GREEN}✓${NC}"
                PASSED=$((PASSED + 1))
            else
                echo -e "${RED}✗${NC} (expected '$expected', got: $output)"
                FAILED=$((FAILED + 1))
            fi
        else
            echo -e "${GREEN}✓${NC}"
            PASSED=$((PASSED + 1))
        fi
    else
        echo -e "${RED}✗${NC} (command failed)"
        FAILED=$((FAILED + 1))
    fi
}

# Detect current provider
echo -e "${BLUE}Quick Environment Test${NC}"
echo "========================"

# Get provider from vm.json
PROVIDER=$(./vm.sh exec "cat /tmp/vm-config.json 2>/dev/null | jq -r .provider || echo unknown" 2>/dev/null | tr -d '\n')
if [ "$PROVIDER" = "unknown" ] || [ -z "$PROVIDER" ]; then
    # Try to detect based on environment
    if ./vm.sh exec "test -f /.dockerenv && echo docker || echo vagrant" 2>/dev/null | grep -q docker; then
        PROVIDER="docker"
    else
        PROVIDER="vagrant"
    fi
fi

echo "Provider: $PROVIDER"
echo ""

# Basic tests
echo "Basic Environment:"
test_command "User" "whoami" "vagrant"
test_command "Shell" "echo \$SHELL" "/bin/zsh"
test_command "Working directory" "pwd" "/workspace"
test_command "Hostname" "hostname -f" ""

echo ""
echo "Development Tools:"
test_command "Node.js" "node --version" "v"
test_command "npm" "npm --version" "[0-9]"
test_command "pnpm" "pnpm --version" "[0-9]"
test_command "Git" "git --version" "git version"

echo ""
echo "Services (if enabled):"
# Check if PostgreSQL is enabled
if ./vm.sh exec "systemctl is-active postgresql 2>/dev/null || docker ps 2>/dev/null | grep postgres" &>/dev/null; then
    test_command "PostgreSQL" "psql --version" "psql"
    test_command "PostgreSQL localhost" "psql -h localhost -U postgres -c 'SELECT 1' -t postgres 2>/dev/null" "1"
fi

# Check if Redis is enabled
if ./vm.sh exec "systemctl is-active redis-server 2>/dev/null || docker ps 2>/dev/null | grep redis" &>/dev/null; then
    test_command "Redis" "redis-cli --version" "redis"
    test_command "Redis localhost" "redis-cli -h localhost ping" "PONG"
fi

# Check if MongoDB is enabled
if ./vm.sh exec "systemctl is-active mongod 2>/dev/null || docker ps 2>/dev/null | grep mongo" &>/dev/null; then
    test_command "MongoDB" "mongosh --version" "[0-9]"
    test_command "MongoDB localhost" "mongosh --eval 'db.runCommand({ping:1})' --quiet" "1"
fi

echo ""
echo "File Sync:"
TIMESTAMP=$(date +%s)
echo "test-$TIMESTAMP" > test-sync-check.txt
sleep 1
test_command "Host→VM sync" "cat /workspace/test-sync-check.txt 2>/dev/null" "test-$TIMESTAMP"
./vm.sh exec "echo 'vm-write-$TIMESTAMP' > /workspace/test-vm-write.txt" &>/dev/null
sleep 1
test_command "VM→Host sync" "cat test-vm-write.txt 2>/dev/null" "vm-write-$TIMESTAMP"
rm -f test-sync-check.txt test-vm-write.txt

echo ""
echo "Terminal Customization:"
test_command "Zsh syntax highlighting" "test -f /usr/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh && echo found" "found"
test_command "Git in prompt" "grep -q git_branch_name ~/.zshrc && echo found" "found"

# Summary
echo ""
echo "========================"
echo -e "Tests passed: ${GREEN}$PASSED${NC}"
echo -e "Tests failed: ${RED}$FAILED${NC}"

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}❌ Some tests failed${NC}"
    exit 1
fi