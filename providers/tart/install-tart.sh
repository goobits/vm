#!/bin/bash
# Tart installation helper for Apple Silicon Macs

set -e

install_tart() {
    echo "🍎 Installing Tart for Apple Silicon Macs..."
    echo ""
    
    # Check if on Apple Silicon Mac
    if [[ "$(uname -s)" != "Darwin" ]]; then
        echo "❌ This system is not running macOS"
        echo "   Current OS: $(uname -s)"
        exit 1
    fi
    
    if [[ "$(uname -m)" != "arm64" ]]; then
        echo "❌ Tart requires Apple Silicon Mac (M1/M2/M3)"
        echo "   Current architecture: $(uname -m)"
        echo ""
        echo "💡 For Intel Macs, consider using:"
        echo "   - Docker provider for Linux containers"
        echo "   - Vagrant provider for full VMs"
        exit 1
    fi
    
    # Check if Tart is already installed
    if command -v tart >/dev/null 2>&1; then
        echo "✅ Tart is already installed"
        echo "   Version: $(tart --version)"
        echo ""
        echo "💡 To update Tart:"
        echo "   brew upgrade cirruslabs/cli/tart"
        exit 0
    fi
    
    # Install via Homebrew
    if ! command -v brew >/dev/null 2>&1; then
        echo "📦 Homebrew is not installed"
        echo ""
        echo "Would you like to install Homebrew first? (y/N): "
        read -r response
        if [[ "$response" =~ ^[yY] ]]; then
            echo "Installing Homebrew..."
            /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
            
            # Add Homebrew to PATH for Apple Silicon
            echo "eval \"\$(/opt/homebrew/bin/brew shellenv)\"" >> ~/.zprofile
            eval "$(/opt/homebrew/bin/brew shellenv)"
        else
            echo "❌ Cannot install Tart without Homebrew"
            echo ""
            echo "💡 To install Homebrew manually:"
            echo "   /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
            exit 1
        fi
    fi
    
    echo "📦 Installing Tart via Homebrew..."
    brew install cirruslabs/cli/tart
    
    echo ""
    echo "✅ Tart installed successfully!"
    echo "   Version: $(tart --version)"
    echo ""
    echo "📚 Quick start guide:"
    echo ""
    echo "1. Create a macOS VM:"
    echo "   vm create --provider tart"
    echo ""
    echo "2. Create a Linux VM with Rosetta:"
    echo "   vm create --provider tart --preset tart-linux"
    echo ""
    echo "3. Use a specific image:"
    echo "   echo 'tart.image: ghcr.io/cirruslabs/debian:latest' >> vm.yaml"
    echo "   vm create --provider tart"
    echo ""
    echo "📖 Available images:"
    echo "   • macOS Sonoma: ghcr.io/cirruslabs/macos-sonoma-base:latest"
    echo "   • macOS Ventura: ghcr.io/cirruslabs/macos-ventura-base:latest"
    echo "   • Ubuntu: ghcr.io/cirruslabs/ubuntu:latest"
    echo "   • Debian: ghcr.io/cirruslabs/debian:latest"
    echo ""
    echo "📖 Documentation:"
    echo "   • Tart: https://github.com/cirruslabs/tart"
    echo "   • VM Tool: Run 'vm --help' for more options"
}

# Show usage if wrong arguments
if [[ "$1" == "--help" ]] || [[ "$1" == "-h" ]]; then
    echo "Tart Installation Helper"
    echo ""
    echo "Usage: $0"
    echo ""
    echo "This script will install Tart on Apple Silicon Macs."
    echo "Tart provides native virtualization using Apple's Virtualization.framework."
    exit 0
fi

# Run installation
install_tart