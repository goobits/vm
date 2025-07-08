#!/bin/bash
# Safe CLI test runner that uses workspace subdirectory

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create test workspace directory
TEST_WORKSPACE="/workspace/.test-runs"
mkdir -p "$TEST_WORKSPACE"

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Running CLI Tests${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Test vm init
echo -e "\n${BLUE}Test 1: vm init command${NC}"
TEST_DIR="$TEST_WORKSPACE/init-test-$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

/workspace/vm.sh init
if [ -f "vm.json" ]; then
    PROJECT_NAME=$(jq -r '.project.name' vm.json)
    if [ "$PROJECT_NAME" = "init-test-$$" ]; then
        echo -e "${GREEN}✓ vm init created customized config${NC}"
    else
        echo -e "${RED}✗ Project name not customized${NC}"
    fi
else
    echo -e "${RED}✗ vm.json not created${NC}"
fi

# Test vm validate
echo -e "\n${BLUE}Test 2: vm validate command${NC}"
cd "$TEST_DIR"
if /workspace/vm.sh validate > /dev/null 2>&1; then
    echo -e "${GREEN}✓ vm validate succeeds with valid config${NC}"
else
    echo -e "${RED}✗ vm validate failed${NC}"
fi

# Test vm validate with invalid config
echo -e "\n${BLUE}Test 3: vm validate with invalid config${NC}"
echo '{"invalid": "config"}' > "$TEST_DIR/bad.json"
if ! /workspace/vm.sh --config "$TEST_DIR/bad.json" validate > /dev/null 2>&1; then
    echo -e "${GREEN}✓ vm validate detects invalid config${NC}"
else
    echo -e "${RED}✗ vm validate should fail with invalid config${NC}"
fi

# Test vm list
echo -e "\n${BLUE}Test 4: vm list command${NC}"
if /workspace/vm.sh list > /dev/null 2>&1; then
    echo -e "${GREEN}✓ vm list runs successfully${NC}"
else
    echo -e "${RED}✗ vm list failed${NC}"
fi

# Test vm help
echo -e "\n${BLUE}Test 5: vm help command${NC}"
if /workspace/vm.sh help | grep -q "Usage:"; then
    echo -e "${GREEN}✓ vm help shows usage information${NC}"
else
    echo -e "${RED}✗ vm help failed${NC}"
fi

# Test --config flag variations
echo -e "\n${BLUE}Test 6: --config flag handling${NC}"
cd "$TEST_DIR"
mkdir -p subdir
cp vm.json subdir/custom.json

# Test with explicit path
if /workspace/vm.sh --config "$TEST_DIR/subdir/custom.json" validate > /dev/null 2>&1; then
    echo -e "${GREEN}✓ --config with explicit path works${NC}"
else
    echo -e "${RED}✗ --config with explicit path failed${NC}"
fi

# Test with directory path (expects vm.json in the directory)
cp vm.json subdir/vm.json
if /workspace/vm.sh --config "$TEST_DIR/subdir" validate > /dev/null 2>&1; then
    echo -e "${GREEN}✓ --config with directory path works${NC}"
else
    echo -e "${RED}✗ --config with directory path failed${NC}"
fi

# Cleanup
echo -e "\n${BLUE}Cleaning up test directories...${NC}"
rm -rf "$TEST_WORKSPACE"

echo -e "\n${GREEN}CLI tests completed!${NC}"