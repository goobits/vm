#!/bin/bash

# Build Artifact Cleanup Script
# Safely removes Rust build artifacts to free up disk space

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RUST_DIR="$PROJECT_ROOT/rust"

echo "🧹 Cleaning up Rust build artifacts..."
echo "Project root: $PROJECT_ROOT"
echo "Rust directory: $RUST_DIR"

if [ ! -d "$RUST_DIR" ]; then
    echo "❌ Rust directory not found at $RUST_DIR"
    exit 1
fi

# Function to safely remove directory and report space saved
safe_remove() {
    local dir="$1"
    if [ -d "$dir" ]; then
        local size=$(du -sh "$dir" 2>/dev/null | cut -f1 || echo "unknown")
        echo "  Removing $dir (size: $size)..."
        rm -rf "$dir"
        echo "  ✅ Removed $dir"
    else
        echo "  ℹ️  Directory $dir does not exist"
    fi
}

echo ""
echo "Removing target directories..."

# Remove main target directory
safe_remove "$RUST_DIR/target"

# Remove cross-compilation target directories
safe_remove "$RUST_DIR/target-linux-aarch64"
safe_remove "$RUST_DIR/target-macos-aarch64"
safe_remove "$RUST_DIR/target-linux-x86_64"
safe_remove "$RUST_DIR/target-macos-x86_64"
safe_remove "$RUST_DIR/target-windows-x86_64"

# Remove any other target-* directories
for target_dir in "$RUST_DIR"/target-*; do
    if [ -d "$target_dir" ]; then
        safe_remove "$target_dir"
    fi
done

# Also clean up any nested target directories in workspace members
echo ""
echo "Checking for nested target directories..."
find "$RUST_DIR" -name "target" -type d -not -path "$RUST_DIR/target" 2>/dev/null | while read -r nested_target; do
    if [ -d "$nested_target" ]; then
        safe_remove "$nested_target"
    fi
done

echo ""
echo "🎉 Build artifact cleanup complete!"
echo ""
echo "To rebuild when needed:"
echo "  cd $RUST_DIR"
echo "  cargo build"
echo "  cargo build --release"
echo ""
echo "To clean up again in the future:"
echo "  $SCRIPT_DIR/clean-build-artifacts.sh"