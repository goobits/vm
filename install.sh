#!/bin/bash
# Global Installation Script for VM Infrastructure
# Usage: ./install.sh

set -e
set -u

# Colors for better output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

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

echo "🚀 Installing VM Infrastructure..."
echo "📂 Installing from: $SCRIPT_DIR"
echo ""

# Check for required dependencies: Rust and Cargo
echo "🔍 Checking dependencies..."
if ! command_exists rustc || ! command_exists cargo; then
    echo -e "${YELLOW}⚠️  Rust is not installed (required for VM tool)${NC}"
    echo ""

    # Offer to install automatically
    echo -n "Would you like to install Rust automatically? (y/N): "
    read -r INSTALL_RUST

    if [[ "$INSTALL_RUST" =~ ^[Yy]$ ]]; then
        echo "📦 Installing Rust..."

        # Install Rust using rustup
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

        # Source the Rust environment
        source "$HOME/.cargo/env" 2>/dev/null || true

        # Verify Rust installation
        if command_exists rustc && command_exists cargo; then
            echo -e "${GREEN}✅ Rust installed successfully${NC}"
        else
            echo -e "${RED}❌ Failed to install Rust${NC}"
            echo ""
            echo "Please install Rust manually:"
            echo -e "${GREEN}  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
            echo ""
            echo "After installing Rust, run this installer again."
            exit 1
        fi
    else
        echo -e "${RED}❌ Rust is required for the VM tool${NC}"
        echo ""
        echo "Install Rust manually:"
        echo -e "${GREEN}  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
        echo ""
        echo "After installing Rust, run this installer again."
        exit 1
    fi
fi

echo -e "${GREEN}✅ Dependencies satisfied${NC}"
echo ""

# Build the vm-config binary
echo "🔧 Building vm-config binary..."
if [[ -d "$SCRIPT_DIR/rust/vm-config" ]]; then
    cd "$SCRIPT_DIR/rust/vm-config"
    if cargo build --release; then
        echo -e "${GREEN}✅ vm-config binary built successfully${NC}"
    else
        echo -e "${RED}❌ Failed to build vm-config binary${NC}"
        exit 1
    fi
    cd "$SCRIPT_DIR"
else
    echo -e "${RED}❌ vm-config source not found at: $SCRIPT_DIR/rust/vm-config${NC}"
    exit 1
fi
echo ""

INSTALL_DIR="${HOME}/.local/share/vm"
BIN_DIR="${HOME}/.local/bin"

# Create directories
mkdir -p "$INSTALL_DIR"
mkdir -p "$BIN_DIR"

# Copy all files except development files
echo "📁 Copying files to $INSTALL_DIR..."

# Check if rsync is available
if command -v rsync &> /dev/null; then
    rsync -av \
        --exclude='.git' \
        --exclude='*.md' \
        --exclude='test' \
        --exclude='install.sh' \
        --exclude='rust/target/' \
        --exclude='rust/*/target/debug' \
        --exclude='rust/*/target/deps' \
        --exclude='rust/*/target/.rustc_info.json' \
        --exclude='rust/*/target/CACHEDIR.TAG' \
        "$SCRIPT_DIR/" "$INSTALL_DIR/"
else
    # Fallback to cp if rsync is not available
    echo "📋 Using cp instead of rsync..."
    # Remove old installation if it exists
    rm -rf "$INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"

    # Copy directories
    for dir in providers shared configs rust lib; do
        if [[ -d "$SCRIPT_DIR/$dir" ]]; then
            cp -r "$SCRIPT_DIR/$dir" "$INSTALL_DIR/"
        fi
    done

    # Copy individual files
    for file in vm.sh validate-config.sh generate-config.sh vm.yaml package.json *.json *.yaml; do
        if [[ -f "$SCRIPT_DIR/$file" ]]; then
            cp "$SCRIPT_DIR/$file" "$INSTALL_DIR/"
        fi
    done

    # Make scripts executable
    chmod +x "$INSTALL_DIR"/*.sh
fi

# Create global vm command
echo "🔗 Creating global 'vm' command in $BIN_DIR..."
cat > "$BIN_DIR/vm" << 'EOF'
#!/bin/bash
# Global VM wrapper - automatically finds vm.yaml in current directory or upward
exec "$HOME/.local/share/vm/vm.sh" "$@"
EOF

chmod +x "$BIN_DIR/vm"

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
