#!/bin/bash
# Main Test Suite Runner

set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Source helper libraries
source "$SCRIPT_DIR/lib/test-helpers.sh"
source "$SCRIPT_DIR/lib/config-generator.sh"

# Test configuration
SUITES="${1:-all}"
PROVIDERS="${2:-docker}"  # Default to docker only for now
VERBOSE="${3:-false}"

# Test results
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
FAILED_TEST_NAMES=()

# Run a test suite
run_test_suite() {
    local suite_path="$1"
    local suite_name=$(basename "$suite_path" .sh)
    
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}Running Test Suite: $suite_name${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    
    # Source the test suite
    source "$suite_path"
    
    # Run all test functions in the suite
    local test_functions=$(declare -F | grep -E "^declare -f test_" | awk '{print $3}')
    
    for test_func in $test_functions; do
        TOTAL_TESTS=$((TOTAL_TESTS + 1))
        
        # Run test in a subshell to isolate failures
        if (
            set -e
            setup_test_env "${suite_name}-${test_func}" "$PROVIDERS"
            run_test "$test_func" "$test_func"
        ); then
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            FAILED_TESTS=$((FAILED_TESTS + 1))
            FAILED_TEST_NAMES+=("${suite_name}::${test_func}")
        fi
        
        # Cleanup is handled by trap in setup_test_env
    done
}

# Check prerequisites
check_prerequisites() {
    echo -e "${BLUE}Checking prerequisites...${NC}"
    
    # Check for vm command
    if ! command -v vm &> /dev/null; then
        # Try using the local vm.sh
        if [ -f "/workspace/vm.sh" ]; then
            alias vm="/workspace/vm.sh"
        else
            echo -e "${RED}❌ vm command not found${NC}"
            exit 1
        fi
    fi
    
    # Check for required tools
    local required_tools=(jq timeout)
    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            echo -e "${RED}❌ Required tool not found: $tool${NC}"
            exit 1
        fi
    done
    
    # Skip provider checks for framework tests
    if [[ "$SUITES" =~ framework ]]; then
        echo -e "${YELLOW}⚠ Skipping provider checks for framework tests${NC}"
        return 0
    fi
    
    # Check provider availability
    case "$PROVIDERS" in
        docker)
            if ! command -v docker &> /dev/null; then
                echo -e "${RED}❌ Docker not installed${NC}"
                exit 1
            fi
            ;;
        vagrant)
            if ! command -v vagrant &> /dev/null; then
                echo -e "${RED}❌ Vagrant not installed${NC}"
                exit 1
            fi
            ;;
        both)
            if ! command -v docker &> /dev/null && ! command -v vagrant &> /dev/null; then
                echo -e "${RED}❌ Neither Docker nor Vagrant installed${NC}"
                exit 1
            fi
            ;;
    esac
    
    echo -e "${GREEN}✓ All prerequisites met${NC}"
}

# Main execution
main() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}VM Test Suite${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo "Testing provider(s): $PROVIDERS"
    echo "Test suite(s): $SUITES"
    echo ""
    
    # Check prerequisites
    check_prerequisites
    
    # Generate test configurations
    echo -e "\n${BLUE}Generating test configurations...${NC}"
    generate_all_configs
    
    # Make vm.sh available as 'vm' command
    export PATH="/workspace:$PATH"
    
    # Run test suites
    if [ "$SUITES" = "all" ]; then
        # Run all test suites
        for suite in "$SCRIPT_DIR"/suites/*.sh; do
            if [ -f "$suite" ]; then
                run_test_suite "$suite"
            fi
        done
    else
        # Run specific test suite
        local suite_path="$SCRIPT_DIR/suites/${SUITES}.sh"
        if [ ! -f "$suite_path" ]; then
            # Try with numeric prefix
            suite_path=$(ls "$SCRIPT_DIR"/suites/*-${SUITES}.sh 2>/dev/null | head -1)
        fi
        if [ -f "$suite_path" ]; then
            run_test_suite "$suite_path"
        else
            echo -e "${RED}Test suite not found: $SUITES${NC}"
            exit 1
        fi
    fi
    
    # Generate final report
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    generate_test_report $PASSED_TESTS $FAILED_TESTS
    
    # Show failed tests if any
    if [ ${#FAILED_TEST_NAMES[@]} -gt 0 ]; then
        echo -e "\n${RED}Failed tests:${NC}"
        for test_name in "${FAILED_TEST_NAMES[@]}"; do
            echo -e "  ${RED}✗ $test_name${NC}"
        done
    fi
    
    # Exit with appropriate code
    [ $FAILED_TESTS -eq 0 ]
}

# Run main function
main