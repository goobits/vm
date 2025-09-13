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

# Check for required dependency: yq
echo "🔍 Checking dependencies..."
if ! command_exists yq; then
    echo -e "${YELLOW}⚠️  yq is not installed (required for VM tool)${NC}"
    echo ""

    # Offer to install automatically
    echo -n "Would you like to install yq automatically? (y/N): "
    read -r INSTALL_YQ

    if [[ "$INSTALL_YQ" =~ ^[Yy]$ ]]; then
        echo "📦 Installing yq..."

        if [[ "$OS" == "macos" ]]; then
            # Try brew first if available
            if command_exists brew; then
                echo "Using Homebrew to install yq..."
                brew install yq
            else
                # Direct download for macOS
                ARCH=$(uname -m)
                if [[ "$ARCH" == "arm64" ]]; then
                    YQ_URL="https://github.com/mikefarah/yq/releases/latest/download/yq_darwin_arm64"
                else
                    YQ_URL="https://github.com/mikefarah/yq/releases/latest/download/yq_darwin_amd64"
                fi

                echo "Downloading yq for macOS ($ARCH)..."
                if command_exists sudo; then
                    sudo curl -L "$YQ_URL" -o /usr/local/bin/yq
                    sudo chmod +x /usr/local/bin/yq
                else
                    # Try user's local bin
                    mkdir -p "$HOME/.local/bin"
                    curl -L "$YQ_URL" -o "$HOME/.local/bin/yq"
                    chmod +x "$HOME/.local/bin/yq"
                fi
            fi
        elif [[ "$OS" == "linux" ]]; then
            # Detect Linux architecture
            ARCH=$(uname -m)
            case "$ARCH" in
                x86_64)  YQ_BINARY="yq_linux_amd64";;
                aarch64) YQ_BINARY="yq_linux_arm64";;
                armv7l)  YQ_BINARY="yq_linux_arm";;
                *)       YQ_BINARY="yq_linux_amd64";;
            esac

            YQ_URL="https://github.com/mikefarah/yq/releases/latest/download/$YQ_BINARY"

            echo "Downloading yq for Linux ($ARCH)..."
            if command_exists sudo; then
                sudo curl -L "$YQ_URL" -o /usr/local/bin/yq
                sudo chmod +x /usr/local/bin/yq
            else
                # Try user's local bin
                mkdir -p "$HOME/.local/bin"
                curl -L "$YQ_URL" -o "$HOME/.local/bin/yq"
                chmod +x "$HOME/.local/bin/yq"
            fi
        fi

        # Verify installation
        if command_exists yq; then
            echo -e "${GREEN}✅ yq installed successfully${NC}"
        else
            echo -e "${RED}❌ Failed to install yq${NC}"
            echo ""
            echo "Please install manually:"
            if [[ "$OS" == "macos" ]]; then
                echo "  brew install yq"
            else
                echo "  Visit: https://github.com/mikefarah/yq/releases"
            fi
            exit 1
        fi
    else
        echo ""
        echo "To install yq manually:"
        if [[ "$OS" == "macos" ]]; then
            echo -e "${GREEN}  brew install yq${NC}"
            echo ""
            echo "Or download directly:"
            ARCH=$(uname -m)
            if [[ "$ARCH" == "arm64" ]]; then
                echo -e "${GREEN}  sudo curl -L https://github.com/mikefarah/yq/releases/latest/download/yq_darwin_arm64 -o /usr/local/bin/yq${NC}"
            else
                echo -e "${GREEN}  sudo curl -L https://github.com/mikefarah/yq/releases/latest/download/yq_darwin_amd64 -o /usr/local/bin/yq${NC}"
            fi
            echo -e "${GREEN}  sudo chmod +x /usr/local/bin/yq${NC}"
        elif [[ "$OS" == "linux" ]]; then
            ARCH=$(uname -m)
            case "$ARCH" in
                x86_64)  YQ_BINARY="yq_linux_amd64";;
                aarch64) YQ_BINARY="yq_linux_arm64";;
                *)       YQ_BINARY="yq_linux_amd64";;
            esac
            echo -e "${GREEN}  sudo curl -L https://github.com/mikefarah/yq/releases/latest/download/$YQ_BINARY -o /usr/local/bin/yq${NC}"
            echo -e "${GREEN}  sudo chmod +x /usr/local/bin/yq${NC}"
        fi
        echo ""
        echo "After installing yq, run this installer again."
        exit 1
    fi
fi

echo -e "${GREEN}✅ Dependencies satisfied${NC}"
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
    rsync -av --exclude='.git' --exclude='*.md' --exclude='test' --exclude='install.sh' "$SCRIPT_DIR/" "$INSTALL_DIR/"
else
    # Fallback to cp if rsync is not available
    echo "📋 Using cp instead of rsync..."
    # Remove old installation if it exists
    rm -rf "$INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"

    # Copy directories
    for dir in providers shared configs; do
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

    # Detect shell and provide specific instructions
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        zsh)
            RC_FILE="~/.zshrc"
            ;;
        bash)
            RC_FILE="~/.bashrc"
            ;;
        *)
            RC_FILE="~/.bashrc or ~/.zshrc"
            ;;
    esac

    echo ""
    echo "To use the 'vm' command, add this to your $RC_FILE:"
    echo -e "${GREEN}    export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"
    echo ""
    echo "Then reload your shell:"
    echo -e "${GREEN}    source $RC_FILE${NC}"
    echo ""
    echo "Or for immediate use, run:"
    echo -e "${GREEN}    export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"
else
    echo -e "${GREEN}✅ $BIN_DIR is already in your PATH${NC}"
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
