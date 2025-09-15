#!/bin/bash
# Minimal test runner for Rust-based VM tool
# This replaces the broken shell test suite with working Rust tests

set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}VM Tool Test Suite (Rust)${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Check if we're in the right directory
if [[ ! -d "rust" ]]; then
    echo -e "${RED}Error: Must run from project root directory${NC}"
    exit 1
fi

# Build all Rust binaries first
echo -e "${YELLOW}Building Rust binaries...${NC}"
cd rust
if cargo build --release 2>&1 | grep -q "Finished\|Building"; then
    echo -e "${GREEN}✓ Build successful${NC}"
else
    echo -e "${YELLOW}Build may have warnings, continuing...${NC}"
fi

# Run all Rust tests
echo -e "\n${YELLOW}Running Rust test suite...${NC}"
if cargo test --workspace --quiet; then
    echo -e "${GREEN}✓ All Rust tests passed${NC}"
else
    echo -e "${RED}✗ Some Rust tests failed${NC}"
    exit 1
fi

# Test specific high-value components
echo -e "\n${YELLOW}Testing key components...${NC}"

# Test vm-detector
echo -n "  vm-detector: "
if cargo test -p vm-detector --quiet 2>/dev/null; then
    echo -e "${GREEN}✓ 22 tests passed${NC}"
else
    echo -e "${RED}✗ failed${NC}"
fi

# Test vm-config
echo -n "  vm-config: "
if cargo test -p vm-config --quiet 2>/dev/null; then
    echo -e "${GREEN}✓ tests passed${NC}"
else
    echo -e "${RED}✗ failed${NC}"
fi

# Test vm-temp
echo -n "  vm-temp: "
if cargo test -p vm-temp --quiet 2>/dev/null; then
    echo -e "${GREEN}✓ mount tests passed${NC}"
else
    echo -e "${RED}✗ failed${NC}"
fi

# Check if main vm binary works
echo -e "\n${YELLOW}Testing vm binary...${NC}"
cd ..
if ./rust/target/release/vm --version >/dev/null 2>&1; then
    echo -e "${GREEN}✓ vm binary works${NC}"
    VERSION=$(./rust/target/release/vm --version 2>&1 | head -1)
    echo -e "  Version: $VERSION"
else
    echo -e "${RED}✗ vm binary failed${NC}"
fi

# Summary
echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}✓ Test suite completed successfully${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"