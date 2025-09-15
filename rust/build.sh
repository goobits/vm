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
PLATFORM_RELEASE_DIR="$PLATFORM_TARGET_DIR/release"

echo "Building VM Tool Rust binaries for platform: $PLATFORM"
echo "Target directory: $PLATFORM_TARGET_DIR"

# Create platform-specific directory and required subdirectories
mkdir -p "$PLATFORM_TARGET_DIR"
mkdir -p "$PLATFORM_RELEASE_DIR/deps"

# Check if we need to build (incremental build detection)
needs_build=true
if [[ -d "$PLATFORM_RELEASE_DIR" ]] && ls "$PLATFORM_RELEASE_DIR"/vm-* >/dev/null 2>&1; then
    # Find the newest binary
    newest_binary=$(ls -t "$PLATFORM_RELEASE_DIR"/vm-* 2>/dev/null | head -1)

    if [[ -n "$newest_binary" ]]; then
        # Check if any Rust source files are newer than the newest binary
        if ! find . -name "*.rs" -newer "$newest_binary" | grep -q .; then
            echo "✅ All binaries are up-to-date for $PLATFORM"
            needs_build=false
        else
            echo "🔄 Source changes detected, rebuilding..."
        fi
    fi
else
    echo "🔨 No platform-specific binaries found, building..."
fi

# Build only if necessary
if [[ "$needs_build" == "true" ]]; then
    echo "🔨 Building workspace..."
    CARGO_TARGET_DIR="$PLATFORM_TARGET_DIR" cargo build --release --workspace
else
    echo "⚡ Skipping build - no changes detected"
fi

echo ""
echo "✅ Build complete! Platform-specific binaries are in: $PLATFORM_TARGET_DIR/release/"
echo "Available binaries:"
if ls "$PLATFORM_TARGET_DIR/release/vm-"* >/dev/null 2>&1; then
    ls -la "$PLATFORM_TARGET_DIR/release/vm-"* 2>/dev/null | grep -v "\.d$" | awk '{print "  - "$NF}'
else
    echo "  (No binaries found)"
fi

# Check specifically for vm-pkg
if [[ -f "$PLATFORM_TARGET_DIR/release/vm-pkg" ]]; then
    echo "  - vm-pkg (unified package manager)"
fi

# Also maintain symlinks in the legacy location for compatibility
echo ""
echo "🔗 Creating compatibility symlinks in target/release/..."
mkdir -p "$SCRIPT_DIR/target/release"

# Handle vm-* binaries
for binary in "$PLATFORM_TARGET_DIR/release/vm-"*; do
    if [[ -f "$binary" && ! "$binary" =~ \.d$ ]]; then
        binary_name=$(basename "$binary")
        ln -sf "../../$PLATFORM/release/$binary_name" "$SCRIPT_DIR/target/release/$binary_name"
        echo "  - $binary_name -> ../target/$PLATFORM/release/$binary_name"
    fi
done

# Handle the main vm binary
if [[ -f "$PLATFORM_TARGET_DIR/release/vm" ]]; then
    ln -sf "../../$PLATFORM/release/vm" "$SCRIPT_DIR/target/release/vm"
    echo "  - vm -> ../target/$PLATFORM/release/vm"
fi

# Also handle vm-pkg if it exists (doesn't start with vm-)
if [[ -f "$PLATFORM_TARGET_DIR/release/vm-pkg" ]]; then
    ln -sf "../../$PLATFORM/release/vm-pkg" "$SCRIPT_DIR/target/release/vm-pkg"
    echo "  - vm-pkg -> ../target/$PLATFORM/release/vm-pkg"
fi