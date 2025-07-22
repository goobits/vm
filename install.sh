#!/bin/bash
# Global Installation Script for VM Infrastructure
# Usage: ./install.sh

set -e

INSTALL_DIR="${HOME}/.local/share/vm"
BIN_DIR="${HOME}/.local/bin"

echo "🚀 Installing VM Infrastructure globally..."

# Create directories
mkdir -p "$INSTALL_DIR"
mkdir -p "$BIN_DIR"

# Copy all files except development files
echo "📁 Copying files to $INSTALL_DIR..."
rsync -av --exclude='.git' --exclude='*.md' --exclude='test' --exclude='examples' --exclude='install.sh' . "$INSTALL_DIR/"

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