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

# Initialize vm-config path
VM_TOOL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VM_CONFIG="$VM_TOOL_DIR/rust/target/release/vm-config"

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
    # No external tools required anymore - vm-config handles everything
    echo -n "  Checking vm-config... "
    if [[ -x "$VM_CONFIG" ]]; then
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${RED}✗${NC} vm-config not found"
        echo "    💡 Build with: cd rust/vm-config && cargo build --release"
        CHECKS_PASSED=false
    fi
}

# Check Ansible playbook syntax
check_ansible_syntax() {
    echo -n "  Checking Ansible playbook syntax... "
    local playbook_path="$1/shared/ansible/playbook.yml"
    if [[ -f "$playbook_path" ]]; then
        # Do a basic YAML syntax check
        if ! "$VM_CONFIG" validate "$playbook_path" >/dev/null 2>&1; then
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
        local pip_packages=$("$VM_CONFIG" transform vm.yaml 'pip_packages[]' --format lines 2>/dev/null || true)
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

# Port conflict checking now handled by vm-ports binary

# Main execution
main() {
    local script_dir="$1"

    check_docker
    check_docker_resources
    check_required_tools
    check_ansible_syntax "$script_dir"
    check_vm_config
    # Port conflicts checked during vm create via vm-ports binary

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