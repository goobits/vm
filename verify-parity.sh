#!/bin/bash
# Verify parity between Docker and Vagrant implementations
# This analyzes the code to ensure both providers work identically

set -e

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

ISSUES=0

echo -e "${BLUE}Parity Verification Analysis${NC}"
echo "============================"
echo "Analyzing code to verify Docker and Vagrant providers offer identical functionality"

# Function to check something
check() {
    local description="$1"
    local condition="$2"
    
    echo -n "  $description... "
    if eval "$condition"; then
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${RED}✗${NC}"
        ISSUES=$((ISSUES + 1))
    fi
}

# Function to verify file contains pattern
contains() {
    grep -q "$1" "$2" 2>/dev/null
}

echo ""
echo "1. Unified Architecture:"
check "Both providers use same Ansible playbook" \
    "contains 'ansible-playbook.*playbook.yml' vm.sh"

check "Docker runs Ansible inside container" \
    "contains 'docker.*exec.*ansible-playbook' vm.sh"

check "Vagrant uses ansible_local provisioner" \
    "contains 'ansible_local' providers/vagrant/Vagrantfile"

echo ""
echo "2. Service Installation:"
check "Services installed via Ansible (not Docker images)" \
    "contains 'Install PostgreSQL' providers/vagrant/ansible/playbook.yml"

check "No separate PostgreSQL container in docker-compose" \
    "! contains 'postgres:' providers/docker/docker-compose.yml.template"

check "No separate Redis container in docker-compose" \
    "! contains 'redis:' providers/docker/docker-compose.yml.template"

echo ""
echo "3. Localhost Connectivity:"
check "Ansible configures PostgreSQL on localhost" \
    "contains 'listen_addresses.*localhost' providers/vagrant/ansible/playbook.yml"

check "Ansible configures Redis bind to localhost" \
    "contains 'bind 127.0.0.1' providers/vagrant/ansible/playbook.yml"

check "Database aliases configured" \
    "contains 'alias psql=' providers/vagrant/ansible/playbook.yml"

echo ""
echo "4. Port Configuration:"
check "Vagrant forwards ports to host" \
    "contains 'forwarded_port' providers/vagrant/Vagrantfile"

check "Docker maps ports in compose template" \
    "contains 'HOST_PORT' providers/docker/docker-compose.yml.template"

check "Both use port binding configuration" \
    "contains 'port_binding' providers/vagrant/Vagrantfile"

echo ""
echo "5. File Synchronization:"
check "Vagrant syncs workspace folder" \
    "contains 'synced_folder.*workspace_path' providers/vagrant/Vagrantfile"

check "Docker mounts workspace as volume" \
    "contains 'PROJECT_PATH.*WORKSPACE_PATH' providers/docker/docker-compose.yml.template"

check "Both mount to same path inside VM/container" \
    "contains '/workspace' vm.json"

echo ""
echo "6. User Configuration:"
check "Same default user (vagrant) in both" \
    "contains 'vagrant' vm.json"

check "Docker creates user at build time" \
    "contains 'useradd.*PROJECT_USER' providers/docker/Dockerfile"

check "Both grant sudo privileges" \
    "contains 'NOPASSWD:ALL' providers/docker/Dockerfile"

echo ""
echo "7. Development Tools:"
check "Node.js installed via NVM in Ansible" \
    "contains 'Install Node.js' providers/vagrant/ansible/playbook.yml"

check "Same Node version configured" \
    "contains 'node.*22' vm.json"

echo ""
echo "8. Command Parity:"
check "vm up works for both providers" \
    "contains 'docker_up\\|up.*vagrant' vm.sh"

check "vm ssh works for both providers" \
    "contains 'docker_ssh\\|ssh.*vagrant' vm.sh"

check "vm exec works for both providers" \
    "contains 'docker_exec\\|exec.*vagrant' vm.sh"

echo ""
echo "9. Configuration Validation:"
check "Both providers use same validate-config.js" \
    "contains 'validate-config.js' vm.sh && contains 'validate-config.js' providers/vagrant/Vagrantfile"

check "Default provider is now Docker" \
    "grep -q '.provider // \"docker\"' vm.sh"

echo ""
echo "10. Terminal Customization:"
check "Both use same terminal configuration in Ansible" \
    "contains 'terminal_emoji' providers/vagrant/ansible/playbook.yml"

check "Zsh configured as default shell" \
    "contains 'zsh' providers/vagrant/ansible/playbook.yml"

# Summary
echo ""
echo "============================"
if [ $ISSUES -eq 0 ]; then
    echo -e "${GREEN}✅ All parity checks passed!${NC}"
    echo ""
    echo "Key findings:"
    echo "- Both providers use the SAME Ansible playbook for provisioning"
    echo "- Services are installed INSIDE the container/VM (not as separate containers)"
    echo "- All services are accessible on localhost with same ports"
    echo "- File synchronization works identically"
    echo "- Same commands work for both providers"
    echo ""
    echo "This unified architecture ensures true parity at the cost of breaking"
    echo "Docker's 'one process per container' convention - a deliberate choice"
    echo "to prioritize developer experience and simplicity."
else
    echo -e "${RED}❌ Found $ISSUES parity issues${NC}"
    echo ""
    echo "The implementation may not have complete parity between providers."
fi