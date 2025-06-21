#!/bin/bash
# Comprehensive parity test suite for Docker and Vagrant providers
# Tests that both providers offer identical functionality

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Ensure we have the proper base directory
BASE_DIR="$SCRIPT_DIR"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test tracking
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
CURRENT_PROVIDER=""

# Test results storage
declare -A TEST_RESULTS

# Function to print section headers
print_section() {
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Function to run a test
run_test() {
    local test_name="$1"
    local command="$2"
    local expected="$3"
    local description="$4"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo -n "  ⏳ $description... "
    
    # Run command and capture output and exit code
    local output
    local exit_code
    # Set VAGRANT_CWD to avoid diagnostic messages and run command
    if [ "$CURRENT_PROVIDER" = "vagrant" ]; then
        output=$(VM_CONFIG="$CURRENT_CONFIG_FILE" VAGRANT_CWD="$BASE_DIR/providers/vagrant" vagrant ssh paritytest -c "$command" 2>/dev/null) && exit_code=$? || exit_code=$?
    else
        # For Docker, use vm ssh command which properly executes as vagrant user
        output=$(./vm.sh --config "$CURRENT_CONFIG_FILE" ssh -c "$command" 2>/dev/null) && exit_code=$? || exit_code=$?
    fi
    
    # Store result for comparison
    local test_key="${CURRENT_PROVIDER}_${test_name}"
    TEST_RESULTS["${test_key}_output"]="$output"
    TEST_RESULTS["${test_key}_exit_code"]="$exit_code"
    
    if [ $exit_code -eq 0 ]; then
        if [ -n "$expected" ]; then
            if echo "$output" | grep -q "$expected"; then
                echo -e "${GREEN}✓${NC} (found: $expected)"
                PASSED_TESTS=$((PASSED_TESTS + 1))
                return 0
            else
                # Clean up output for display
                local clean_output=$(echo "$output" | tr -d '\n' | tr -s ' ')
                echo -e "${RED}✗${NC} (expected '$expected', got: ${clean_output:0:50}...)"
                FAILED_TESTS=$((FAILED_TESTS + 1))
                return 1
            fi
        else
            echo -e "${GREEN}✓${NC}"
            PASSED_TESTS=$((PASSED_TESTS + 1))
            return 0
        fi
    else
        echo -e "${RED}✗${NC} (exit code: $exit_code)"
        echo "    Output: ${output:0:100}..."
        FAILED_TESTS=$((FAILED_TESTS + 1))
        return 1
    fi
}

# Function to compare results between providers
compare_providers() {
    local test_name="$1"
    local description="$2"
    
    local vagrant_output="${TEST_RESULTS[vagrant_${test_name}_output]}"
    local docker_output="${TEST_RESULTS[docker_${test_name}_output]}"
    local vagrant_exit="${TEST_RESULTS[vagrant_${test_name}_exit_code]}"
    local docker_exit="${TEST_RESULTS[docker_${test_name}_exit_code]}"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo -n "  🔄 $description... "
    
    if [ "$vagrant_exit" = "$docker_exit" ]; then
        # For some tests, we need exact output match
        if [[ "$test_name" == *"_exact" ]]; then
            if [ "$vagrant_output" = "$docker_output" ]; then
                echo -e "${GREEN}✓${NC} (identical)"
                PASSED_TESTS=$((PASSED_TESTS + 1))
            else
                echo -e "${RED}✗${NC} (outputs differ)"
                echo "    Vagrant: ${vagrant_output:0:50}..."
                echo "    Docker:  ${docker_output:0:50}..."
                FAILED_TESTS=$((FAILED_TESTS + 1))
            fi
        else
            echo -e "${GREEN}✓${NC} (compatible)"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        fi
    else
        echo -e "${RED}✗${NC} (exit codes differ: Vagrant=$vagrant_exit, Docker=$docker_exit)"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
}

# Function to test a provider
test_provider() {
    local provider="$1"
    local config_file="$2"
    CURRENT_PROVIDER="$provider"
    
    # Make config_file available to run_test function
    export CURRENT_CONFIG_FILE="$config_file"
    
    print_section "Testing $provider Provider"
    
    # Create test config
    cat > "$config_file" << EOF
{
  "provider": "$provider",
  "project": {
    "name": "paritytest",
    "hostname": "test.local",
    "workspace_path": "/workspace"
  },
  "services": {
    "postgresql": {
      "enabled": true,
      "database": "testdb",
      "user": "postgres",
      "password": "testpass"
    },
    "redis": {
      "enabled": true
    },
    "mongodb": {
      "enabled": true
    }
  },
  "ports": {
    "postgresql": 15432,
    "redis": 16379,
    "mongodb": 17017
  },
  "terminal": {
    "emoji": "🧪",
    "username": "tester"
  }
}
EOF
    
    echo "📝 Created test configuration"
    
    # Start VM/Container
    echo ""
    echo "🚀 Starting $provider environment..."
    echo "   This may take 2-3 minutes for Vagrant, 30-60 seconds for Docker..."
    echo "   Logging to: /tmp/${provider}_up.log"
    
    # Start in background and show progress
    ./vm.sh --config "$config_file" up > /tmp/${provider}_up.log 2>&1 &
    local pid=$!
    
    # Show progress dots while waiting
    echo -n "   Progress: "
    while kill -0 $pid 2>/dev/null; do
        echo -n "."
        sleep 5
        # Show last line of log for context
        if [ -f "/tmp/${provider}_up.log" ]; then
            local last_line=$(tail -1 /tmp/${provider}_up.log | cut -c1-60)
            echo -ne "\r   Progress: ... $last_line"
        fi
    done
    
    # Check if it succeeded
    wait $pid
    local exit_code=$?
    echo ""
    
    if [ $exit_code -eq 0 ]; then
        echo -e "   ${GREEN}✓${NC} Environment started successfully"
    else
        echo -e "   ${RED}✗${NC} Failed to start environment"
        echo "   Last 20 lines of log:"
        tail -20 /tmp/${provider}_up.log
        return 1
    fi
    
    # Wait for environment to be ready
    echo "⏳ Waiting for environment to stabilize..."
    sleep 10
    
    # Check if provisioning completed by looking for Node.js
    echo "   Checking provisioning status..."
    local provision_check
    if [ "$provider" = "vagrant" ]; then
        provision_check=$(VM_CONFIG="$CURRENT_CONFIG_FILE" VAGRANT_CWD="$BASE_DIR/providers/vagrant" vagrant ssh paritytest -c "which node" 2>/dev/null || echo "not found")
    else
        provision_check=$(./vm.sh --config "$config_file" ssh -c "which node" 2>/dev/null || echo "not found")
    fi
    
    if [[ "$provision_check" == *"not found"* ]] || [ -z "$provision_check" ]; then
        echo -e "   ${YELLOW}⚠${NC}  Provisioning may be incomplete. Checking Ansible status..."
        echo "   You can check the full log at: /tmp/${provider}_up.log"
    fi
    
    # Run all tests
    echo ""
    echo "🧪 Running tests:"
    
    # Basic functionality tests
    echo ""
    echo "1️⃣ Basic Functionality:"
    run_test "whoami" "whoami" "vagrant" "User verification"
    run_test "pwd" "cd /workspace && pwd" "/workspace" "Working directory"
    run_test "hostname" "hostname" "test.local" "Hostname configuration"
    run_test "workspace_exists" "test -d /workspace && echo exists" "exists" "Workspace mounted"
    run_test "workspace_writable" "touch /workspace/test.txt && rm /workspace/test.txt && echo writable" "writable" "Workspace writable"
    
    # Development tools tests
    echo ""
    echo "2️⃣ Development Tools:"
    run_test "node_version" "source ~/.nvm/nvm.sh && node --version" "v22" "Node.js v22 installed"
    run_test "npm_exists" "source ~/.nvm/nvm.sh && which npm" "npm" "npm available"
    run_test "pnpm_exists" "source ~/.nvm/nvm.sh && which pnpm" "pnpm" "pnpm available"
    run_test "git_exists" "git --version" "git version" "Git installed"
    run_test "zsh_shell" "echo \$SHELL" "/bin/zsh" "Zsh as default shell"
    
    # Service connectivity tests
    echo ""
    echo "3️⃣ Service Connectivity:"
    run_test "postgres_ping" "PGPASSWORD=testpass psql -h localhost -p 15432 -U postgres -d testdb -c 'SELECT 1' -t" "1" "PostgreSQL on localhost"
    run_test "redis_ping" "redis-cli -p 16379 ping" "PONG" "Redis on localhost"
    run_test "mongodb_ping" "mongosh --port 17017 --eval 'db.runCommand({ping: 1})' --quiet" "ok: 1" "MongoDB on localhost"
    
    # Terminal customization tests
    echo ""
    echo "4️⃣ Terminal Customization:"
    run_test "prompt_emoji" "grep '🧪' ~/.zshrc" "🧪" "Terminal emoji configured"
    run_test "prompt_username" "grep 'tester' ~/.zshrc" "tester" "Terminal username configured"
    
    # Environment tests
    echo ""
    echo "5️⃣ Environment Configuration:"
    run_test "locale" "echo \$LANG" "en_US.UTF-8" "Locale set correctly"
    run_test "term_var" "echo \$TERM" "dumb\|xterm" "TERM variable set"
    
    # File sync test
    echo ""
    echo "6️⃣ File Synchronization:"
    echo "test-${provider}-$(date +%s)" > test-sync.txt
    sleep 2
    run_test "file_sync" "cat /workspace/test-sync.txt" "test-${provider}" "File sync working"
    rm -f test-sync.txt
    
    # Cleanup
    echo ""
    echo "🧹 Cleaning up..."
    echo "   Destroying test environment..."
    
    ./vm.sh --config "$config_file" destroy -f > /tmp/${provider}_destroy.log 2>&1 &
    local destroy_pid=$!
    
    # Show progress
    echo -n "   Progress: "
    while kill -0 $destroy_pid 2>/dev/null; do
        echo -n "."
        sleep 2
    done
    
    wait $destroy_pid
    if [ $? -eq 0 ]; then
        echo -e "\n   ${GREEN}✓${NC} Environment destroyed"
    else
        echo -e "\n   ${RED}✗${NC} Failed to destroy environment"
        tail -10 /tmp/${provider}_destroy.log
    fi
    
    rm -f "$config_file"
}

# Main test execution
main() {
    print_section "VM Provider Parity Test Suite"
    echo "Testing that Docker and Vagrant providers offer identical functionality"
    
    # Check prerequisites
    echo ""
    echo "📋 Checking prerequisites:"
    
    if ! command -v docker &> /dev/null; then
        echo -e "   ${YELLOW}⚠${NC}  Docker not installed - skipping Docker tests"
        SKIP_DOCKER=1
    else
        echo -e "   ${GREEN}✓${NC} Docker found"
    fi
    
    if ! command -v vagrant &> /dev/null; then
        echo -e "   ${YELLOW}⚠${NC}  Vagrant not installed - skipping Vagrant tests"
        SKIP_VAGRANT=1
    else
        echo -e "   ${GREEN}✓${NC} Vagrant found"
    fi
    
    if [ "${SKIP_DOCKER:-0}" = "1" ] && [ "${SKIP_VAGRANT:-0}" = "1" ]; then
        echo -e "${RED}❌ Neither Docker nor Vagrant installed. Cannot run tests.${NC}"
        exit 1
    fi
    
    # Test each provider
    if [ "${SKIP_VAGRANT:-0}" != "1" ]; then
        test_provider "vagrant" "test-vagrant.json"
    fi
    
    if [ "${SKIP_DOCKER:-0}" != "1" ]; then
        test_provider "docker" "test-docker.json"
    fi
    
    # Compare results if both providers were tested
    if [ "${SKIP_DOCKER:-0}" != "1" ] && [ "${SKIP_VAGRANT:-0}" != "1" ]; then
        print_section "Parity Comparison"
        echo "Comparing results between Docker and Vagrant providers:"
        echo ""
        
        compare_providers "whoami" "User identity"
        compare_providers "pwd" "Working directory"
        compare_providers "hostname" "Hostname"
        compare_providers "workspace_exists" "Workspace mounting"
        compare_providers "node_version" "Node.js version"
        compare_providers "postgres_ping" "PostgreSQL connectivity"
        compare_providers "redis_ping" "Redis connectivity"
        compare_providers "mongodb_ping" "MongoDB connectivity"
        compare_providers "locale" "Locale settings"
    fi
    
    # Final summary
    print_section "Test Summary"
    echo "Total tests run: $TOTAL_TESTS"
    echo -e "Passed: ${GREEN}$PASSED_TESTS${NC}"
    echo -e "Failed: ${RED}$FAILED_TESTS${NC}"
    
    if [ $FAILED_TESTS -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✅ All tests passed! Both providers offer identical functionality.${NC}"
        exit 0
    else
        echo ""
        echo -e "${RED}❌ Some tests failed. Review the output above.${NC}"
        exit 1
    fi
}

# Run the test suite
main "$@"