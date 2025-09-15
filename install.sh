#!/bin/bash
# Global Installation Script for VM Infrastructure
# Usage: ./install.sh [--clean]

set -e
set -u

# Colors for better output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse command line arguments
CLEAN_BUILD=false
for arg in "$@"; do
    case $arg in
        --clean)
            CLEAN_BUILD=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [--clean]"
            echo ""
            echo "Options:"
            echo "  --clean    Clean all build artifacts before building"
            echo "  --help     Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $arg${NC}"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Verify we're in the right directory
if [[ ! -f "$SCRIPT_DIR/vm.sh" ]]; then
    echo -e "${RED}❌ Error: Cannot find vm.sh in $SCRIPT_DIR${NC}"
    echo "💡 Make sure you're running install.sh from the vm directory"
    exit 1
fi

# Function to check if command exists
command_exists() {
    command -v "$1" &> /dev/null
}

# Function to detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux";;
        Darwin*)    echo "macos";;
        *)          echo "unknown";;
    esac
}

OS=$(detect_os)

# Function to detect platform (matches build.sh logic)
detect_platform() {
    local os arch
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)
    case "$arch" in
        x86_64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) arch="unknown" ;;
    esac
    echo "${os}-${arch}"
}

# Function to install Rust
install_rust() {
    echo -e "${YELLOW}⚠️  Rust is not installed (required for VM tool)${NC}"
    echo ""
    echo "The VM tool is written in Rust for performance and safety."
    echo "You can install it using your system's package manager or the official Rust installer (rustup)."
    echo ""

    local install_cmd=""
    local pkg_manager=""

    if [[ "$OS" == "linux" ]]; then
        if command_exists apt-get; then
            pkg_manager="apt"
            install_cmd="sudo apt-get update && sudo apt-get install -y cargo"
        elif command_exists dnf; then
            pkg_manager="dnf"
            install_cmd="sudo dnf install -y cargo"
        elif command_exists pacman; then
            pkg_manager="pacman"
            install_cmd="sudo pacman -S --noconfirm rust"
        fi
    fi

    echo -n "Would you like to install Rust automatically? (y/N): "
    read -r INSTALL_RUST

    if [[ ! "$INSTALL_RUST" =~ ^[Yy]$ ]]; then
        echo -e "${RED}❌ Rust is required. Please install it manually.${NC}"
        if [[ -n "$pkg_manager" ]]; then
            echo "   Recommended command for your system ($pkg_manager):"
            echo -e "   ${GREEN}$install_cmd${NC}"
        fi
        echo "   Alternatively, use the official installer:"
        echo -e "   ${GREEN}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
        exit 1
    fi

    if [[ -n "$pkg_manager" ]]; then
        echo "📦 Installing Rust using $pkg_manager..."
        if eval "$install_cmd"; then
            echo -e "${GREEN}✅ Rust installed successfully via $pkg_manager.${NC}"
        else
            echo -e "${RED}❌ Failed to install Rust using $pkg_manager.${NC}"
            echo "   Attempting fallback to rustup..."
            install_rust_with_rustup
        fi
    else
        install_rust_with_rustup
    fi
}

# Function to install rust using rustup
install_rust_with_rustup() {
    echo "📦 Installing Rust using rustup (official installer)..."
    if curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; then
        # Source the Rust environment for the current session
        source "$HOME/.cargo/env" 2>/dev/null || true
        echo -e "${GREEN}✅ Rust installed successfully via rustup.${NC}"
    else
        echo -e "${RED}❌ Failed to install Rust using rustup.${NC}"
        echo "Please try installing it manually:"
        echo -e "   ${GREEN}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
        exit 1
    fi
}

echo "🚀 Installing VM Infrastructure..."
echo "📂 Installing from: $SCRIPT_DIR"
echo ""

# Check for required dependencies: Rust and Cargo
echo "🔍 Checking dependencies..."
if ! command_exists rustc || ! command_exists cargo; then
    install_rust
fi

# Verify Rust installation again after potential installation
if ! command_exists rustc || ! command_exists cargo; then
    echo -e "${RED}❌ Rust installation could not be verified.${NC}"
    echo "Please ensure 'rustc' and 'cargo' are in your PATH and run the installer again."
    exit 1
fi

echo -e "${GREEN}✅ Dependencies satisfied${NC}"
echo ""

# Clean build artifacts if requested
if [[ "$CLEAN_BUILD" == "true" ]]; then
    echo "🧹 Cleaning build artifacts..."
    if (cd "$SCRIPT_DIR/rust" && cargo clean); then
        echo -e "${GREEN}✅ Build artifacts cleaned.${NC}"

        # Also remove platform-specific directories (only in rust/target)
        # Safety: Only removes build artifacts in the target directory
        if [[ -d "$SCRIPT_DIR/rust/target" ]]; then
            # List what will be removed for transparency
            dirs_to_remove=$(find "$SCRIPT_DIR/rust/target" -maxdepth 1 -type d \( -name "darwin-*" -o -name "linux-*" \) 2>/dev/null || true)
            if [[ -n "$dirs_to_remove" ]]; then
                echo "  Removing platform-specific build directories:"
                echo "$dirs_to_remove" | while IFS= read -r dir; do
                    if [[ -n "$dir" ]] && [[ -d "$dir" ]]; then
                        echo "    - $(basename "$dir")"
                        rm -rf "$dir"
                    fi
                done
                echo -e "${GREEN}✅ Platform-specific directories removed.${NC}"
            else
                echo "  No platform-specific directories found to remove."
            fi
        fi
    else
        echo -e "${YELLOW}⚠️  Warning: Failed to clean some build artifacts, continuing anyway...${NC}"
    fi
    echo ""
fi

# Build the Rust binaries
echo "🔧 Building Rust binaries..."
PLATFORM=$(detect_platform)
PLATFORM_TARGET_DIR="$SCRIPT_DIR/rust/target/$PLATFORM"
if (cd "$SCRIPT_DIR/rust" && CARGO_TARGET_DIR="$PLATFORM_TARGET_DIR" cargo build --release --workspace); then
    echo -e "${GREEN}✅ Rust binaries built successfully.${NC}"
else
    echo -e "${RED}❌ Failed to build Rust binaries.${NC}"
    exit 1
fi
echo ""

BIN_DIR="${HOME}/.local/bin"
mkdir -p "$BIN_DIR"

# Create a direct symbolic link to the compiled binary
PLATFORM=$(detect_platform)
PLATFORM_BINARY="$SCRIPT_DIR/rust/target/$PLATFORM/release/vm"
FALLBACK_BINARY="$SCRIPT_DIR/rust/target/release/vm"

# Use platform-specific binary if it exists, otherwise fall back to generic location
if [[ -f "$PLATFORM_BINARY" ]]; then
    SOURCE_BINARY="$PLATFORM_BINARY"
else
    SOURCE_BINARY="$FALLBACK_BINARY"
fi

LINK_NAME="$BIN_DIR/vm"

echo "🔗 Creating global 'vm' command..."
# Remove existing link or file if it exists
rm -f "$LINK_NAME"
# Create the new symbolic link
ln -s "$SOURCE_BINARY" "$LINK_NAME"
echo "✅ Symlink created: $LINK_NAME -> $SOURCE_BINARY"

# Check if ~/.local/bin is in PATH
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo -e "${YELLOW}⚠️  $BIN_DIR is not in your PATH${NC}"

    # Detect shell and update config file
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        zsh)
            RC_FILE="$HOME/.zshrc"
            ;;
        bash)
            RC_FILE="$HOME/.bashrc"
            ;;
        fish)
            RC_FILE="$HOME/.config/fish/config.fish"
            ;;
        *)
            RC_FILE="$HOME/.bashrc"
            ;;
    esac

    echo ""
    echo -n "Would you like to add $BIN_DIR to your PATH automatically? (y/N): "
    read -r ADD_TO_PATH

    if [[ "$ADD_TO_PATH" =~ ^[Yy]$ ]]; then
        # Check if PATH export already exists in file
        if [[ "$SHELL_NAME" == "fish" ]]; then
            # Fish shell syntax
            if ! grep -q "fish_add_path.*\.local/bin" "$RC_FILE" 2>/dev/null; then
                echo "" >> "$RC_FILE"
                echo "# Added by VM tool installer" >> "$RC_FILE"
                echo "fish_add_path -p \$HOME/.local/bin" >> "$RC_FILE"
                echo -e "${GREEN}✅ Added PATH to $RC_FILE${NC}"
            else
                echo -e "${YELLOW}PATH entry already exists in $RC_FILE${NC}"
            fi
        else
            # Bash/Zsh syntax
            if ! grep -q "\.local/bin" "$RC_FILE" 2>/dev/null; then
                echo "" >> "$RC_FILE"
                echo "# Added by VM tool installer" >> "$RC_FILE"
                echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$RC_FILE"
                echo -e "${GREEN}✅ Added PATH to $RC_FILE${NC}"
            else
                echo -e "${YELLOW}PATH entry already exists in $RC_FILE${NC}"
            fi
        fi

        # Also export for current session
        export PATH="$HOME/.local/bin:$PATH"

        echo ""
        echo -e "${GREEN}The 'vm' command is now available!${NC}"
        echo "Note: New terminal windows will have the vm command available."
    else
        echo ""
        echo "To use the 'vm' command, add this to your $RC_FILE:"
        if [[ "$SHELL_NAME" == "fish" ]]; then
            echo -e "${GREEN}    fish_add_path -p \$HOME/.local/bin${NC}"
        else
            echo -e "${GREEN}    export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"
        fi
        echo ""
        echo "Then reload your shell:"
        echo -e "${GREEN}    source $RC_FILE${NC}"
    fi
else
    echo -e "${GREEN}✅ $BIN_DIR is already in your PATH${NC}"

    # Double-check it's in shell config for persistence
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        zsh)
            RC_FILE="$HOME/.zshrc"
            ;;
        bash)
            RC_FILE="$HOME/.bashrc"
            ;;
        *)
            RC_FILE="$HOME/.bashrc"
            ;;
    esac

    if ! grep -q "\.local/bin" "$RC_FILE" 2>/dev/null; then
        echo -e "${YELLOW}Note: PATH is set for this session but not in $RC_FILE${NC}"
        echo -n "Add to $RC_FILE for permanent access? (y/N): "
        read -r ADD_PERMANENT

        if [[ "$ADD_PERMANENT" =~ ^[Yy]$ ]]; then
            echo "" >> "$RC_FILE"
            echo "# Added by VM tool installer" >> "$RC_FILE"
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$RC_FILE"
            echo -e "${GREEN}✅ Added to $RC_FILE for permanent access${NC}"
        fi
    fi
fi

echo ""
echo -e "${GREEN}🎉 Installation complete!${NC}"
echo ""
echo "Quick start:"
echo -e "  ${GREEN}vm create${NC}    # Create and start VM based on your project"
echo -e "  ${GREEN}vm ssh${NC}       # Connect to VM"
echo -e "  ${GREEN}vm destroy${NC}   # Delete VM"
echo ""
echo "The 'vm' command will detect your project type and configure the right environment."
echo "For more commands: vm --help"
