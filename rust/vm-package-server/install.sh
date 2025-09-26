#!/bin/bash
# Install script for Goobits Package Server

set -e

# Configuration
BINARY_NAME="pkg-server"
BINARY_SOURCE_PATH="./target/release/pkg-server"
DEFAULT_INSTALL_DIR="/usr/local/bin"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Cleanup function for trap handler
cleanup() {
    local exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo -e "\n${RED}❌ Installation failed. Cleaning up...${NC}"
        # Remove partially installed binary if it exists
        if [ -n "$INSTALL_DIR" ] && [ -f "$INSTALL_DIR/$BINARY_NAME" ]; then
            echo -e "${YELLOW}Removing partially installed binary from $INSTALL_DIR...${NC}"
            rm -f "$INSTALL_DIR/$BINARY_NAME" 2>/dev/null || true
        fi
    fi
    exit $exit_code
}

# Set trap for cleanup on failure
trap cleanup EXIT INT TERM

# Function to determine the best installation directory
determine_install_dir() {
    # Allow override via environment variable
    if [ -n "$PKG_SERVER_INSTALL_DIR" ]; then
        echo "$PKG_SERVER_INSTALL_DIR"
        return
    fi

    # Check if running as root for system-wide install
    if [ "$EUID" -eq 0 ]; then
        echo "$DEFAULT_INSTALL_DIR"
        return
    fi

    # For non-root installs, prefer ~/.cargo/bin (already in PATH for most dev environments)
    # Fall back to ~/.local/bin if cargo bin doesn't exist
    if [ -d "$HOME/.cargo/bin" ]; then
        echo "$HOME/.cargo/bin"
    else
        echo "$HOME/.local/bin"
    fi
}

# Function to normalize path (system-agnostic)
normalize_path() {
    local path="$1"
    # Remove trailing slash
    path="${path%/}"

    # Try to resolve symlinks - use different approaches for different systems
    if command -v realpath >/dev/null 2>&1; then
        # GNU coreutils (Linux) or installed on macOS
        realpath "$path" 2>/dev/null || echo "$path"
    elif [ "$(uname)" = "Darwin" ] && command -v greadlink >/dev/null 2>&1; then
        # GNU readlink on macOS (from coreutils)
        greadlink -f "$path" 2>/dev/null || echo "$path"
    elif command -v readlink >/dev/null 2>&1; then
        # Try readlink with -f flag (may not work on macOS)
        readlink -f "$path" 2>/dev/null || echo "$path"
    else
        # Fallback: just return the path as-is
        echo "$path"
    fi
}

# Function to check if a directory is in PATH with improved logic
is_in_path() {
    local dir="$1"
    # Normalize the directory path
    local normalized_dir
    normalized_dir=$(normalize_path "$dir")

    # Check each PATH component
    local IFS=':'
    for path_dir in $PATH; do
        # Skip empty path components
        [ -z "$path_dir" ] && continue

        # Normalize path component
        local normalized_path
        normalized_path=$(normalize_path "$path_dir")

        # Compare normalized paths
        if [ "$normalized_dir" = "$normalized_path" ]; then
            return 0
        fi
    done
    return 1
}

# Function to build the binary if needed
build_binary() {
    echo -e "${YELLOW}Building for current platform...${NC}"

    if ! command -v cargo >/dev/null 2>&1; then
        echo -e "${RED}❌ Cargo not found. Please install Rust and Cargo first.${NC}"
        echo "Visit https://rustup.rs/ to install Rust"
        return 1
    fi

    # Build natively for current platform
    echo -e "${YELLOW}Running: cargo build --release${NC}"
    if ! cargo build --release; then
        echo -e "${RED}❌ Build failed${NC}"
        return 1
    fi

    echo -e "${GREEN}✅ Build completed successfully${NC}"
    return 0
}

# Function to validate binary source
validate_binary() {
    # Check if binary exists
    if [ ! -f "$BINARY_SOURCE_PATH" ]; then
        echo -e "${YELLOW}⚠️  Binary not found, building from source...${NC}"
        if ! build_binary; then
            return 1
        fi
    else
        # Verify it's for the correct platform
        if command -v file >/dev/null 2>&1; then
            local file_info
            file_info=$(file "$BINARY_SOURCE_PATH")
            local needs_rebuild=false

            # Detect current platform
            case "$(uname -s)" in
                Darwin*)
                    if [[ ! "$file_info" =~ "Mach-O" ]]; then
                        echo -e "${YELLOW}⚠️  Binary is not for macOS, rebuilding...${NC}"
                        needs_rebuild=true
                    fi
                    ;;
                Linux*)
                    if [[ ! "$file_info" =~ "ELF" ]]; then
                        echo -e "${YELLOW}⚠️  Binary is not for Linux, rebuilding...${NC}"
                        needs_rebuild=true
                    fi
                    ;;
            esac

            if [ "$needs_rebuild" = true ]; then
                if ! build_binary; then
                    return 1
                fi
            fi
        fi
    fi

    # Final check
    if [ ! -f "$BINARY_SOURCE_PATH" ]; then
        echo -e "${RED}❌ Binary still not found after build${NC}"
        return 1
    fi

    if [ ! -x "$BINARY_SOURCE_PATH" ]; then
        echo -e "${RED}❌ Binary at $BINARY_SOURCE_PATH is not executable${NC}"
        return 1
    fi

    return 0
}

# Function to install the binary
install_binary() {
    local install_dir="$1"
    local target_path="$install_dir/$BINARY_NAME"

    echo -e "${YELLOW}Copying binary to $install_dir...${NC}"

    # Create directory if it doesn't exist
    mkdir -p "$install_dir"

    # Check if binary already exists and might be in use
    if [ -f "$target_path" ]; then
        # Try to stop any running instances
        if command -v pkill >/dev/null 2>&1; then
            pkill -f "$BINARY_NAME" 2>/dev/null || true
            sleep 1  # Give processes time to exit
        fi

        # Use a safer copy method: copy to temp file then move
        local temp_file="$target_path.tmp.$$"
        cp "$BINARY_SOURCE_PATH" "$temp_file"
        chmod +x "$temp_file"

        # Move the temp file to the final location (atomic operation)
        mv -f "$temp_file" "$target_path"
    else
        # Simple copy if file doesn't exist
        cp "$BINARY_SOURCE_PATH" "$target_path"
        chmod +x "$target_path"
    fi
}

# Function to detect the current shell config file
detect_shell_config() {
    # Check SHELL environment variable first
    local current_shell
    current_shell=$(basename "${SHELL:-/bin/bash}")

    case "$current_shell" in
        zsh)
            # Check for .zshrc
            if [ -f "$HOME/.zshrc" ]; then
                echo "$HOME/.zshrc"
            else
                echo "$HOME/.zshrc (create this file)"
            fi
            ;;
        bash)
            # Check for .bash_profile or .bashrc
            if [ -f "$HOME/.bash_profile" ]; then
                echo "$HOME/.bash_profile"
            elif [ -f "$HOME/.bashrc" ]; then
                echo "$HOME/.bashrc"
            else
                echo "$HOME/.bashrc (create this file)"
            fi
            ;;
        fish)
            echo "$HOME/.config/fish/config.fish"
            ;;
        *)
            echo "your shell config file (.bashrc, .zshrc, etc.)"
            ;;
    esac
}

# Function to provide PATH setup instructions
provide_path_instructions() {
    local install_dir="$1"
    local shell_config
    shell_config=$(detect_shell_config)

    echo -e "${YELLOW}⚠️  $install_dir is not in your PATH${NC}"
    echo ""

    case "$install_dir" in
        "$HOME/.local/bin")
            echo "Add this line to $shell_config:"
            echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
            ;;
        "$HOME/.cargo/bin")
            echo "This is unusual - ~/.cargo/bin should be in PATH if you have Rust installed."
            echo "You may need to reinstall Rust or add this line to $shell_config:"
            echo "export PATH=\"\$HOME/.cargo/bin:\$PATH\""
            ;;
        *)
            echo "Add this line to $shell_config:"
            echo "export PATH=\"$install_dir:\$PATH\""
            ;;
    esac

    echo ""
    echo "Then reload your shell with: source $shell_config"
    echo "Or restart your terminal."
}

# Main installation function
main() {
    echo -e "${GREEN}📦 Installing Goobits Package Server...${NC}"

    # Validate binary exists and is executable
    if ! validate_binary; then
        exit 1
    fi

    # Determine installation directory
    INSTALL_DIR=$(determine_install_dir)

    if [ "$EUID" -ne 0 ] && [ "$INSTALL_DIR" != "$DEFAULT_INSTALL_DIR" ]; then
        echo -e "${YELLOW}Not running as root. Installing to $INSTALL_DIR${NC}"
    fi

    # Install the binary
    install_binary "$INSTALL_DIR"

    # Check if directory is in PATH
    if ! is_in_path "$INSTALL_DIR"; then
        provide_path_instructions "$INSTALL_DIR"
    else
        echo -e "${GREEN}✅ $INSTALL_DIR is already in your PATH${NC}"
    fi

    echo -e "${GREEN}✅ Installation complete!${NC}"
    echo ""
    echo "You can now run: $BINARY_NAME --help"
    echo "Or start the server: $BINARY_NAME start"
}

# Run main function
main

