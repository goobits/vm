#!/bin/bash
# Test the current environment setup

set -e

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "Testing Current Environment"
echo "==========================="

# Basic checks
echo ""
echo "Environment:"
echo -n "  Current directory: "
pwd

echo -n "  User: "
whoami

echo -n "  Shell: "
echo $SHELL

echo -n "  Node.js: "
node --version || echo "not found"

echo -n "  Git: "
git --version || echo "not found"

# Check VM tool
echo ""
echo "VM Tool:"
echo -n "  vm.sh exists: "
test -f vm.sh && echo -e "${GREEN}✓${NC}" || echo -e "${RED}✗${NC}"

echo -n "  vm.sh executable: "
test -x vm.sh && echo -e "${GREEN}✓${NC}" || echo -e "${RED}✗${NC}"

echo -n "  validate-config.js exists: "
test -f validate-config.js && echo -e "${GREEN}✓${NC}" || echo -e "${RED}✗${NC}"

# Check providers
echo ""
echo "Provider Files:"
echo -n "  Dockerfile: "
test -f providers/docker/Dockerfile && echo -e "${GREEN}✓${NC}" || echo -e "${RED}✗${NC}"

echo -n "  docker-compose template: "
test -f providers/docker/docker-compose.yml.template && echo -e "${GREEN}✓${NC}" || echo -e "${RED}✗${NC}"

echo -n "  Vagrantfile: "
test -f providers/vagrant/Vagrantfile && echo -e "${GREEN}✓${NC}" || echo -e "${RED}✗${NC}"

echo -n "  Ansible playbook: "
test -f providers/vagrant/ansible/playbook.yml && echo -e "${GREEN}✓${NC}" || echo -e "${RED}✗${NC}"

# Test configuration validation
echo ""
echo "Configuration:"
echo -n "  Default vm.json valid: "
if ./vm.sh validate >/dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    ./vm.sh validate
fi

echo -n "  Test config valid: "
if ./vm.sh --config test-vagrant.json validate >/dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
fi

echo ""
echo "Ready to run parity tests!"