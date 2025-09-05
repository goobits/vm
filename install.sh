#!/bin/bash
# Global Installation Script for VM Infrastructure
# Usage: ./install.sh

set -e
set -u

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Verify we're in the right directory
if [[ ! -f "$SCRIPT_DIR/vm.sh" ]]; then
    echo "❌ Error: Cannot find vm.sh in $SCRIPT_DIR"
    echo "💡 Make sure you're running install.sh from the vm directory"
    exit 1
fi

INSTALL_DIR="${HOME}/.local/share/vm"
BIN_DIR="${HOME}/.local/bin"

echo "🚀 Installing VM Infrastructure globally..."
echo "📂 Installing from: $SCRIPT_DIR"

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
    echo "⚠️  $BIN_DIR is not in your PATH."
    echo "Add this to your ~/.bashrc or ~/.zshrc:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    echo "Or run now:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
else
    echo "✅ $BIN_DIR is already in your PATH"
fi

echo ""
echo "🎉 Installation complete!"
echo ""
echo "Usage:"
echo "  vm create    # Create and start VM (looks for vm.yaml in current dir or upward)"
echo "  vm ssh       # Connect to VM"
echo "  vm validate  # Check configuration"
echo "  vm halt      # Stop VM"
echo "  vm destroy   # Delete VM"
echo ""
echo "The 'vm' command will automatically search for vm.yaml in:"
echo "  1. Current directory"
echo "  2. Parent directory"
echo "  3. Grandparent directory"
echo "  4. Fall back to defaults if none found"
