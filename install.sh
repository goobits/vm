#!/bin/bash
# Installation Script for VM Infrastructure
# This script builds and installs the 'vm' tool from the local source code.

set -e
set -u

# --- Colors ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# --- Helper Functions ---
fail() {
    echo -e "${RED}❌ Error: $1${NC}" >&2
    exit 1
}

command_exists() {
    command -v "$1" &>/dev/null
}

echo "🚀 Installing VM Infrastructure from source..."

# 1. Check for required dependencies (cargo).
if ! command_exists cargo; then
    echo -e "${YELLOW}⚠️  Cargo not found. Installing Rust toolchain...${NC}"

    # Auto-install Rust
    if command_exists curl; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    elif command_exists wget; then
        wget -qO- https://sh.rustup.rs | sh -s -- -y
    else
        fail "Neither 'curl' nor 'wget' found. Cannot auto-install Rust. Please install manually: https://rustup.rs"
    fi

    # Source the cargo environment
    if [[ -f "$HOME/.cargo/env" ]]; then
        source "$HOME/.cargo/env"
    else
        fail "Failed to locate Rust environment after installation"
    fi

    # Verify installation
    if ! command_exists cargo; then
        fail "Rust installation completed but 'cargo' is still not available. Please check your PATH"
    fi

    echo -e "${GREEN}✅ Rust toolchain installed successfully${NC}"
fi

# 2. Get the directory where this script is located to reliably find the Cargo.toml.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST_PATH="$SCRIPT_DIR/rust/Cargo.toml"

if [[ ! -f "$MANIFEST_PATH" ]]; then
    fail "Could not find the Rust workspace at '$MANIFEST_PATH'. Please run this script from within the project directory."
fi

# 3. Execute the Rust installer, passing along all script arguments.
# The Rust installer handles the build, symlinking, and PATH configuration.
echo "🔧 Invoking the Rust installer..."
if cargo run --package vm-installer --manifest-path "$MANIFEST_PATH" -- "$@"; then
    # The Rust installer now prints its own success messages.
    # A final confirmation is still useful here.
    echo -e "\n${GREEN}✅ The Rust installer completed successfully.${NC}"
else
    fail "The Rust installer encountered an error."
fi