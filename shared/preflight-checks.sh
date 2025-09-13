#!/bin/bash
# Pre-flight checks for VM creation
# This script validates the environment before attempting to create a VM

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🔍 Running pre-flight checks..."

# Track if any checks fail
CHECKS_PASSED=true

# Check Docker is running
check_docker() {
    echo -n "  Checking Docker... "
    if ! docker info >/dev/null 2>&1; then
        echo -e "${RED}✗${NC} Docker is not running"
        echo "    💡 Start Docker Desktop or run: sudo systemctl start docker"
        CHECKS_PASSED=false
    else
        echo -e "${GREEN}✓${NC}"
    fi
}

# Check Docker resources
check_docker_resources() {
    echo -n "  Checking Docker resources... "
    local disk_usage=$(docker system df --format "{{.Reclaimable}}" | head -n1 | sed 's/[^0-9.]//g')
    if [[ -n "$disk_usage" ]] && (( $(echo "$disk_usage > 10" | bc -l 2>/dev/null || echo 0) )); then
        echo -e "${YELLOW}⚠${NC} ${disk_usage}GB reclaimable space"
        echo "    💡 Consider running: docker system prune"
    else
        echo -e "${GREEN}✓${NC}"
    fi
}

# Check for required tools
check_required_tools() {
    local tools=("yq" "jq")
    for tool in "${tools[@]}"; do
        echo -n "  Checking $tool... "
        if ! command -v "$tool" >/dev/null 2>&1; then
            echo -e "${RED}✗${NC} $tool not found"
            echo "    💡 Install with: brew install $tool"
            CHECKS_PASSED=false
        else
            echo -e "${GREEN}✓${NC}"
        fi
    done
}

# Check Ansible playbook syntax
check_ansible_syntax() {
    echo -n "  Checking Ansible playbook syntax... "
    local playbook_path="$1/shared/ansible/playbook.yml"
    if [[ -f "$playbook_path" ]]; then
        # Do a basic YAML syntax check
        if ! yq eval '.' "$playbook_path" >/dev/null 2>&1; then
            echo -e "${RED}✗${NC} Invalid YAML syntax"
            echo "    💡 Check $playbook_path for syntax errors"
            CHECKS_PASSED=false
        else
            echo -e "${GREEN}✓${NC}"
        fi
    else
        echo -e "${YELLOW}⚠${NC} Playbook not found (will use installed version)"
    fi
}

# Check vm.yaml for common issues
check_vm_config() {
    echo -n "  Checking vm.yaml configuration... "
    if [[ -f "vm.yaml" ]]; then
        # Check for common package name issues
        local pip_packages=$(yq eval '.pip_packages[]' vm.yaml 2>/dev/null || true)
        if [[ -n "$pip_packages" ]]; then
            while IFS= read -r package; do
                if [[ "$package" == "claudeflow" ]]; then
                    echo -e "${YELLOW}⚠${NC} Package 'claudeflow' doesn't exist in PyPI"
                    echo "    💡 Remove or comment out this package in vm.yaml"
                fi
            done <<< "$pip_packages"
        fi
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${GREEN}✓${NC} (using defaults)"
    fi
}

# Check for port conflicts
check_port_conflicts() {
    echo -n "  Checking for port conflicts... "
    local common_ports=(3000 3001 5000 5432 6379 8000 8080)
    local conflicts=""

    for port in "${common_ports[@]}"; do
        if lsof -i ":$port" >/dev/null 2>&1; then
            conflicts="$conflicts $port"
        fi
    done

    if [[ -n "$conflicts" ]]; then
        echo -e "${YELLOW}⚠${NC} Ports in use:$conflicts"
        echo "    💡 These may conflict if configured in vm.yaml"
    else
        echo -e "${GREEN}✓${NC}"
    fi
}

# Main execution
main() {
    local script_dir="$1"

    check_docker
    check_docker_resources
    check_required_tools
    check_ansible_syntax "$script_dir"
    check_vm_config
    check_port_conflicts

    echo ""
    if [[ "$CHECKS_PASSED" == "true" ]]; then
        echo -e "${GREEN}✅ All pre-flight checks passed!${NC}"
        return 0
    else
        echo -e "${RED}❌ Some pre-flight checks failed${NC}"
        echo "   Fix the issues above before proceeding"
        return 1
    fi
}

# Run checks if this script is executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    main "$SCRIPT_DIR"
fi