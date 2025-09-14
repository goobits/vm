#!/bin/bash
# Build all Rust binaries in the workspace with platform-specific targeting

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Detect current platform
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

PLATFORM=$(detect_platform)
PLATFORM_TARGET_DIR="$SCRIPT_DIR/target/$PLATFORM"

echo "Building VM Tool Rust binaries for platform: $PLATFORM"
echo "Target directory: $PLATFORM_TARGET_DIR"

# Create platform-specific directory
mkdir -p "$PLATFORM_TARGET_DIR"

# Build with platform-specific target directory
echo "🔨 Building workspace..."
CARGO_TARGET_DIR="$PLATFORM_TARGET_DIR" cargo build --release --workspace

echo ""
echo "✅ Build complete! Platform-specific binaries are in: $PLATFORM_TARGET_DIR/release/"
echo "Available binaries:"
if ls "$PLATFORM_TARGET_DIR/release/vm-"* >/dev/null 2>&1; then
    ls -la "$PLATFORM_TARGET_DIR/release/vm-"* 2>/dev/null | grep -v "\.d$" | awk '{print "  - "$NF}'
else
    echo "  (No binaries found)"
fi

# Also maintain symlinks in the legacy location for compatibility
echo ""
echo "🔗 Creating compatibility symlinks in target/release/..."
mkdir -p "$SCRIPT_DIR/target/release"
for binary in "$PLATFORM_TARGET_DIR/release/vm-"*; do
    if [[ -f "$binary" && ! "$binary" =~ \.d$ ]]; then
        binary_name=$(basename "$binary")
        ln -sf "../../$PLATFORM/release/$binary_name" "$SCRIPT_DIR/target/release/$binary_name"
        echo "  - $binary_name -> ../target/$PLATFORM/release/$binary_name"
    fi
done