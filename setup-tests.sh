#!/bin/bash
# Test Environment Setup Script
# Prepares local environment for running tests without global installation
# Usage: ./test-setup.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Test Environment Setup${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Track issues
ISSUES_FOUND=0
WARNINGS_FOUND=0

# 1. Check and build vm-config if needed
echo -e "${BLUE}Checking vm-config binary...${NC}"
if [[ -f "$SCRIPT_DIR/rust/vm-config/target/release/vm-config" ]]; then
    # Test if binary works (might be built for different platform)
    if "$SCRIPT_DIR/rust/vm-config/target/release/vm-config" --version >/dev/null 2>&1; then
        echo -e "${GREEN}✓ vm-config binary exists and works${NC}"
    else
        echo -e "${YELLOW}⚠ vm-config binary exists but doesn't work (wrong platform?)${NC}"
        echo "  Rebuilding for current platform..."

        if command -v cargo >/dev/null 2>&1; then
            cd "$SCRIPT_DIR/rust/vm-config"
            cargo clean --release 2>/dev/null || true
            if cargo build --release; then
                echo -e "${GREEN}✓ vm-config rebuilt successfully${NC}"
            else
                echo -e "${RED}✗ Failed to rebuild vm-config${NC}"
                ISSUES_FOUND=$((ISSUES_FOUND + 1))
            fi
            cd "$SCRIPT_DIR"
        else
            echo -e "${RED}✗ Cargo not found - cannot rebuild vm-config${NC}"
            echo "  Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
            ISSUES_FOUND=$((ISSUES_FOUND + 1))
        fi
    fi
else
    echo -e "${YELLOW}⚠ vm-config binary not found${NC}"

    if command -v cargo >/dev/null 2>&1; then
        echo "  Building vm-config..."
        cd "$SCRIPT_DIR/rust/vm-config"
        if cargo build --release; then
            echo -e "${GREEN}✓ vm-config built successfully${NC}"
        else
            echo -e "${RED}✗ Failed to build vm-config${NC}"
            ISSUES_FOUND=$((ISSUES_FOUND + 1))
        fi
        cd "$SCRIPT_DIR"
    else
        echo -e "${RED}✗ Cargo not found - cannot build vm-config${NC}"
        echo "  Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        ISSUES_FOUND=$((ISSUES_FOUND + 1))
    fi
fi
echo ""

# 2. Check Docker access
echo -e "${BLUE}Checking Docker access...${NC}"
if command -v docker >/dev/null 2>&1; then
    if docker ps >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Docker is accessible without sudo${NC}"
    elif sudo -n docker ps >/dev/null 2>&1; then
        echo -e "${YELLOW}⚠ Docker requires sudo (tests will skip Docker-dependent features)${NC}"
        echo "  To fix: sudo usermod -aG docker \$USER && newgrp docker"
        WARNINGS_FOUND=$((WARNINGS_FOUND + 1))
    else
        echo -e "${YELLOW}⚠ Docker not accessible (tests will skip Docker-dependent features)${NC}"
        echo "  1. Check if Docker daemon is running: sudo systemctl status docker"
        echo "  2. Add user to docker group: sudo usermod -aG docker \$USER"
        echo "  3. Apply changes: newgrp docker (or logout/login)"
        WARNINGS_FOUND=$((WARNINGS_FOUND + 1))
    fi
else
    echo -e "${RED}✗ Docker not installed${NC}"
    echo "  Install Docker: https://docs.docker.com/engine/install/"
    ISSUES_FOUND=$((ISSUES_FOUND + 1))
fi
echo ""

# 3. Check required test dependencies
echo -e "${BLUE}Checking test dependencies...${NC}"
MISSING_DEPS=()

# Check for required commands
for cmd in timeout bash; do
    if command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${GREEN}✓ $cmd found${NC}"
    else
        echo -e "${RED}✗ $cmd not found${NC}"
        MISSING_DEPS+=("$cmd")
        ISSUES_FOUND=$((ISSUES_FOUND + 1))
    fi
done

# Check for optional but useful commands
for cmd in node python3; do
    if command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${GREEN}✓ $cmd found${NC}"
    else
        echo -e "${YELLOW}⚠ $cmd not found (some tests may be limited)${NC}"
        WARNINGS_FOUND=$((WARNINGS_FOUND + 1))
    fi
done

if [[ ${#MISSING_DEPS[@]} -gt 0 ]]; then
    echo ""
    echo -e "${RED}Missing required dependencies: ${MISSING_DEPS[*]}${NC}"
    echo "  Install them using your package manager"
fi
echo ""

# 4. Check test file permissions
echo -e "${BLUE}Checking test file permissions...${NC}"
PERMISSION_ISSUES=0

# Check main test runner
if [[ -x "$SCRIPT_DIR/run-tests.sh" ]]; then
    echo -e "${GREEN}✓ run-tests.sh is executable${NC}"
else
    echo -e "${YELLOW}⚠ run-tests.sh is not executable${NC}"
    chmod +x "$SCRIPT_DIR/run-tests.sh"
    echo -e "${GREEN}  Fixed: chmod +x run-tests.sh${NC}"
fi

# Check test scripts
for test_script in test/**/*.sh; do
    if [[ -f "$test_script" ]] && [[ ! -x "$test_script" ]]; then
        chmod +x "$test_script"
        PERMISSION_ISSUES=$((PERMISSION_ISSUES + 1))
    fi
done

if [[ $PERMISSION_ISSUES -gt 0 ]]; then
    echo -e "${GREEN}✓ Fixed permissions for $PERMISSION_ISSUES test scripts${NC}"
else
    echo -e "${GREEN}✓ All test scripts have correct permissions${NC}"
fi
echo ""

# 5. Quick environment validation
echo -e "${BLUE}Validating environment...${NC}"

# Check if vm.sh exists and is executable
if [[ -f "$SCRIPT_DIR/vm.sh" ]]; then
    if [[ -x "$SCRIPT_DIR/vm.sh" ]]; then
        echo -e "${GREEN}✓ vm.sh exists and is executable${NC}"
    else
        chmod +x "$SCRIPT_DIR/vm.sh"
        echo -e "${GREEN}✓ vm.sh made executable${NC}"
    fi
else
    echo -e "${RED}✗ vm.sh not found${NC}"
    ISSUES_FOUND=$((ISSUES_FOUND + 1))
fi

# Check if test directories exist
if [[ -d "$SCRIPT_DIR/test/unit" ]] && [[ -d "$SCRIPT_DIR/test/integration" ]] && [[ -d "$SCRIPT_DIR/test/system" ]]; then
    echo -e "${GREEN}✓ Test directories exist${NC}"
else
    echo -e "${RED}✗ Test directories missing${NC}"
    ISSUES_FOUND=$((ISSUES_FOUND + 1))
fi

# Check if vm.yaml exists (for tests that need it)
if [[ -f "$SCRIPT_DIR/vm.yaml" ]]; then
    echo -e "${GREEN}✓ vm.yaml configuration exists${NC}"
else
    echo -e "${YELLOW}⚠ vm.yaml not found (some tests may fail)${NC}"
    echo "  Run: ./vm.sh init"
    WARNINGS_FOUND=$((WARNINGS_FOUND + 1))
fi
echo ""

# 6. Summary report
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Setup Summary${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

if [[ $ISSUES_FOUND -eq 0 ]] && [[ $WARNINGS_FOUND -eq 0 ]]; then
    echo -e "${GREEN}✅ Test environment is ready!${NC}"
    echo ""
    echo "Run tests with:"
    echo -e "  ${GREEN}./run-tests.sh${NC}              # Run all tests"
    echo -e "  ${GREEN}./run-tests.sh --suite cli${NC}  # Run specific suite"
    echo -e "  ${GREEN}./run-tests.sh --list${NC}       # List available suites"
elif [[ $ISSUES_FOUND -eq 0 ]]; then
    echo -e "${GREEN}✅ Test environment is ready (with $WARNINGS_FOUND warnings)${NC}"
    echo ""
    echo "Tests can run but some features may be skipped."
    echo "Check warnings above for optional improvements."
    echo ""
    echo "Run tests with:"
    echo -e "  ${GREEN}./run-tests.sh${NC}"
else
    echo -e "${RED}❌ Test environment has $ISSUES_FOUND critical issues${NC}"
    if [[ $WARNINGS_FOUND -gt 0 ]]; then
        echo -e "${YELLOW}   Plus $WARNINGS_FOUND warnings${NC}"
    fi
    echo ""
    echo "Please fix the issues marked with ✗ above."
    echo "Warnings (⚠) are optional but recommended."
    exit 1
fi

echo ""
echo "For more information, see: test/README.md"