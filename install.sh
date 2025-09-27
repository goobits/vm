#!/bin/bash
# Installation Script for VM Infrastructure
# This script builds and installs the 'vm' tool from the local source code.
#
# Usage:
#   ./install.sh                    # Install vm tool only
#   ./install.sh --pkg-server       # Install vm + standalone pkg-server
#   ./install.sh --pkg-server-only  # Install only standalone pkg-server

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

# 3. Parse install options
PKG_SERVER_MODE="none"
INSTALLER_ARGS=()

for arg in "$@"; do
    case $arg in
        --pkg-server)
            PKG_SERVER_MODE="both"
            ;;
        --pkg-server-only)
            PKG_SERVER_MODE="standalone"
            ;;
        *)
            INSTALLER_ARGS+=("$arg")
            ;;
    esac
done

# 4. Build standalone package server if requested
if [[ "$PKG_SERVER_MODE" == "both" || "$PKG_SERVER_MODE" == "standalone" ]]; then
    echo "📦 Building standalone package server..."
    cd "$SCRIPT_DIR/rust"

    if cargo build --release --features standalone-binary -p vm-package-server; then
        echo -e "${GREEN}✅ Standalone package server built successfully${NC}"

        # Install the standalone binary
        PKG_SERVER_BIN="$SCRIPT_DIR/rust/target/release/pkg-server"
        if [[ -f "$PKG_SERVER_BIN" ]]; then
            # Determine install directory
            if [[ -w "/usr/local/bin" ]]; then
                INSTALL_DIR="/usr/local/bin"
            elif [[ -w "$HOME/.local/bin" ]]; then
                INSTALL_DIR="$HOME/.local/bin"
                mkdir -p "$INSTALL_DIR"
            else
                fail "No writable install directory found. Try running with sudo or ensure ~/.local/bin exists"
            fi

            echo "📥 Installing pkg-server to $INSTALL_DIR..."
            cp "$PKG_SERVER_BIN" "$INSTALL_DIR/pkg-server"
            chmod +x "$INSTALL_DIR/pkg-server"
            echo -e "${GREEN}✅ pkg-server installed to $INSTALL_DIR/pkg-server${NC}"
        else
            fail "Built package server binary not found at $PKG_SERVER_BIN"
        fi
    else
        fail "Failed to build standalone package server"
    fi

    cd "$SCRIPT_DIR"
fi

# 5. Skip vm installation if only building standalone package server
if [[ "$PKG_SERVER_MODE" == "standalone" ]]; then
    echo -e "\n${GREEN}✅ Standalone package server installation completed.${NC}"
    echo "🚀 You can now use: pkg-server --help"
    exit 0
fi

# 6. Execute the Rust installer, passing along filtered arguments.
# The Rust installer handles the build, symlinking, and PATH configuration.
echo "🔧 Invoking the Rust installer..."
if cargo run --package vm-installer --manifest-path "$MANIFEST_PATH" -- "${INSTALLER_ARGS[@]}"; then
    # The Rust installer now prints its own success messages.
    # A final confirmation is still useful here.
    echo -e "\n${GREEN}✅ The Rust installer completed successfully.${NC}"

    if [[ "$PKG_SERVER_MODE" == "both" ]]; then
        echo "🚀 You can now use:"
        echo "   vm pkg --help          # Package server via vm CLI"
        echo "   pkg-server --help      # Standalone package server"
    fi
else
    fail "The Rust installer encountered an error."
fi