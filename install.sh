#!/bin/bash
#
# VM Infrastructure Installation Script
#
# Supports: macOS, Ubuntu/Debian, Fedora/RHEL, Arch Linux
# Security: Enterprise-grade with verification and comprehensive error handling
#
# Usage:
#   ./install.sh                    # Build and install vm tool from source
#

set -euo pipefail  # Exit on error, undefined vars, pipe failures
IFS=$'\n\t'       # Secure Internal Field Separator

# ============================================================================
# Configuration Constants
# ============================================================================

# Read version from project Cargo.toml if available
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "$SCRIPT_DIR/rust/Cargo.toml" ]]; then
    SCRIPT_VERSION=$(grep '^version = ' "$SCRIPT_DIR/rust/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')
    readonly SCRIPT_VERSION
else
    readonly SCRIPT_VERSION="unknown"
fi
readonly SCRIPT_NAME="$(basename "$0")"
readonly LOG_PREFIX="🔧 VM Installer"
readonly TIMEOUT_SECONDS=30
readonly CARGO_TIMEOUT_SECONDS=600  # 10 minutes for cargo operations (clean builds take 2-3 minutes)
readonly LOG_FILE="$HOME/.vm-install.log"
readonly REPO_URL="https://github.com/goobits/vm"  # Replace with your repo

# Error codes
readonly ERR_PLATFORM_DETECT=1
readonly ERR_DEPENDENCY_MISSING=2
readonly ERR_NETWORK_TIMEOUT=3
readonly ERR_VERIFICATION_FAILED=4
readonly ERR_INSTALL_FAILED=5
readonly ERR_PATH_CONFIG=6
readonly ERR_PERMISSION_DENIED=7
readonly ERR_CARGO_BUILD=8

# Color codes for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly BLUE='\033[0;34m'
readonly YELLOW='\033[1;33m'
readonly NC='\033[0m' # No Color

# ============================================================================
# Global Variables (set by detection functions)
# ============================================================================

OS_TYPE=""
OS_VERSION=""
ARCH=""
PACKAGE_MANAGER=""
CURRENT_SHELL=""
SHELL_CONFIG=""
SHELL_TYPE=""  # bash, zsh, fish, etc.

# Installation options (parsed from arguments)
INSTALLER_ARGS=()

# ============================================================================
# Logging Functions
# ============================================================================

log_info() {
    echo -e "${BLUE}ℹ️  ${LOG_PREFIX}: $*${NC}"
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] INFO: $*" >> "$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}✅ $*${NC}"
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] SUCCESS: $*" >> "$LOG_FILE"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $*${NC}"
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] WARNING: $*" >> "$LOG_FILE"
}

log_error() {
    echo -e "${RED}❌ $*${NC}" >&2
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] ERROR: $*" >> "$LOG_FILE"

    # System logging if available
    if command -v logger &>/dev/null; then
        logger -t "$SCRIPT_NAME" -p user.err "ERROR: $*"
    fi
}

# ============================================================================
# Error Handling
# ============================================================================

handle_error() {
    local error_code="$1"
    local error_msg="$2"
    local suggested_fix="${3:-Please check the log file at $LOG_FILE}"

    {
        echo -e "${RED}═══════════════════════════════════════════${NC}"
        echo -e "${RED}❌ Error Code: E${error_code}${NC}"
        echo -e "${RED}❌ Message: ${error_msg}${NC}"
        echo -e "${YELLOW}💡 Fix: ${suggested_fix}${NC}"
        echo -e "${BLUE}📍 Debug Info:${NC}"
        echo -e "  Platform: ${OS_TYPE:-unknown} ${OS_VERSION:-unknown}"
        echo -e "  Arch: ${ARCH:-unknown}"
        echo -e "  Shell: ${CURRENT_SHELL:-unknown}"
        echo -e "  Log: $LOG_FILE"
        echo -e "  Time: $(date '+%Y-%m-%d %H:%M:%S')"
        echo -e "${RED}═══════════════════════════════════════════${NC}"
    } >&2

    log_error "E$error_code: $error_msg"
    exit "$error_code"
}

command_exists() {
    command -v "$1" &>/dev/null
}

# ============================================================================
# Platform Detection (Phase 2)
# ============================================================================

detect_platform() {
    log_info "Detecting platform..."

    # Detect architecture
    ARCH=$(uname -m)

    # Detect OS type and version
    if [[ "$OSTYPE" == "darwin"* ]]; then
        OS_TYPE="macos"
        OS_VERSION=$(sw_vers -productVersion 2>/dev/null || echo "unknown")

        # Check for Homebrew
        if command_exists brew; then
            PACKAGE_MANAGER="homebrew"
        else
            PACKAGE_MANAGER="none"
            log_warning "Homebrew not found. Some features may be limited."
        fi

    elif [[ -f /etc/os-release ]]; then
        # Parse os-release file safely
        OS_TYPE=$(grep '^ID=' /etc/os-release | cut -d= -f2 | tr -d '"' | head -1)
        OS_VERSION=$(grep '^VERSION_ID=' /etc/os-release | cut -d= -f2 | tr -d '"' | head -1)

        # Detect package manager based on distribution
        case "$OS_TYPE" in
            ubuntu|debian)
                PACKAGE_MANAGER="apt"
                ;;
            fedora|rhel|centos|rocky|almalinux)
                if command_exists dnf; then
                    PACKAGE_MANAGER="dnf"
                elif command_exists yum; then
                    PACKAGE_MANAGER="yum"
                else
                    PACKAGE_MANAGER="none"
                fi
                ;;
            arch|manjaro|endeavouros)
                PACKAGE_MANAGER="pacman"
                ;;
            opensuse*)
                PACKAGE_MANAGER="zypper"
                ;;
            alpine)
                PACKAGE_MANAGER="apk"
                ;;
            *)
                PACKAGE_MANAGER="none"
                log_warning "Unknown Linux distribution: $OS_TYPE"
                ;;
        esac

    elif [[ -f /etc/redhat-release ]]; then
        # Fallback for older RHEL/CentOS
        OS_TYPE="rhel"
        OS_VERSION=$(rpm -E %{rhel} 2>/dev/null || echo "unknown")
        if command_exists dnf; then
            PACKAGE_MANAGER="dnf"
        elif command_exists yum; then
            PACKAGE_MANAGER="yum"
        else
            PACKAGE_MANAGER="none"
        fi

    else
        OS_TYPE="unknown"
        OS_VERSION="unknown"
        PACKAGE_MANAGER="none"
        log_warning "Unable to detect operating system"
    fi

    log_success "Detected: $OS_TYPE $OS_VERSION ($ARCH) with $PACKAGE_MANAGER"
}

detect_shell_config() {
    log_info "Detecting shell configuration..."

    # Get current shell
    CURRENT_SHELL=$(basename "$SHELL" 2>/dev/null || echo "bash")

    # Determine shell type and config file
    case "$CURRENT_SHELL" in
        zsh)
            SHELL_TYPE="zsh"
            if [[ "$OS_TYPE" == "macos" ]]; then
                # macOS uses .zprofile for login shells
                SHELL_CONFIG="$HOME/.zprofile"
            else
                SHELL_CONFIG="$HOME/.zshrc"
            fi
            ;;

        bash)
            SHELL_TYPE="bash"
            # Check for various bash configs in order of preference
            if [[ "$OS_TYPE" == "macos" ]]; then
                SHELL_CONFIG="$HOME/.bash_profile"
            elif [[ -f "$HOME/.bash_profile" ]]; then
                SHELL_CONFIG="$HOME/.bash_profile"
            elif [[ -f "$HOME/.bashrc" ]]; then
                SHELL_CONFIG="$HOME/.bashrc"
            else
                SHELL_CONFIG="$HOME/.profile"
            fi
            ;;

        fish)
            SHELL_TYPE="fish"
            SHELL_CONFIG="$HOME/.config/fish/config.fish"
            mkdir -p "$(dirname "$SHELL_CONFIG")" 2>/dev/null || true
            ;;

        sh|dash|ash)
            SHELL_TYPE="sh"
            SHELL_CONFIG="$HOME/.profile"
            ;;

        *)
            SHELL_TYPE="unknown"
            SHELL_CONFIG="$HOME/.profile"
            log_warning "Unknown shell: $CURRENT_SHELL, using .profile"
            ;;
    esac

    log_success "Shell: $CURRENT_SHELL (config: $(basename "$SHELL_CONFIG"))"
}

# ============================================================================
# Secure Rust Installation (Phase 1) - Only for source builds
# ============================================================================

verify_rustup_checksum() {
    local file="$1"
    log_info "Verifying installer checksum..."

    # Determine architecture and platform for the correct checksum
    local rust_arch
    local rust_platform

    # Map architecture
    case "$ARCH" in
        x86_64)
            rust_arch="x86_64"
            ;;
        aarch64|arm64)
            rust_arch="aarch64"
            ;;
        *)
            log_error "Unsupported architecture for checksum verification: $ARCH"
            return 1
            ;;
    esac

    # Map platform
    case "$OS_TYPE" in
        macos)
            rust_platform="apple-darwin"
            ;;
        *)
            rust_platform="unknown-linux-gnu"
            ;;
    esac

    local rustup_target="${rust_arch}-${rust_platform}"
    log_info "Fetching checksum for target: $rustup_target"

    # Fetch the official checksum from Rust's release metadata
    local channel_url="https://forge.rust-lang.org/infra/channel-layout.html"
    local checksum_url="https://static.rust-lang.org/rustup/dist/${rustup_target}/rustup-init.sha256"

    local expected_hash
    if ! expected_hash=$(timeout "$TIMEOUT_SECONDS" curl --proto '=https' --tlsv1.2 -sSf "$checksum_url" 2>/dev/null | awk '{print $1}'); then
        log_warning "Could not fetch official checksum from $checksum_url"
        log_warning "Falling back to size verification only"

        # Fallback to basic size check
        local file_size
        file_size=$(stat -f%z "$file" 2>/dev/null || stat -c%s "$file" 2>/dev/null || echo "0")

        if [[ "$file_size" -lt 1000 ]]; then
            log_error "Downloaded file too small ($file_size bytes), likely corrupted"
            return 1
        fi

        log_warning "Size verification passed ($file_size bytes) but checksum not verified"
        return 0
    fi

    if [[ -z "$expected_hash" ]]; then
        log_error "Retrieved empty checksum from $checksum_url"
        return 1
    fi

    # Calculate actual hash of downloaded file
    local actual_hash
    if command_exists sha256sum; then
        actual_hash=$(sha256sum "$file" | cut -d' ' -f1)
    elif command_exists shasum; then
        actual_hash=$(shasum -a 256 "$file" | cut -d' ' -f1)
    elif command_exists openssl; then
        actual_hash=$(openssl dgst -sha256 "$file" | cut -d' ' -f2)
    else
        log_error "No SHA256 tool available (tried sha256sum, shasum, openssl)"
        handle_error $ERR_DEPENDENCY_MISSING \
            "SHA256 checksum tool not found" \
            "Install sha256sum, shasum, or openssl"
        return 1
    fi

    # Compare hashes
    if [[ "$expected_hash" == "$actual_hash" ]]; then
        log_success "SHA256 checksum verification passed"
        log_info "  Hash: ${actual_hash:0:16}..."
        return 0
    else
        log_error "SHA256 checksum verification FAILED!"
        log_error "  Expected: $expected_hash"
        log_error "  Actual:   $actual_hash"
        log_error "  File may be corrupted or tampered with"
        return 1
    fi
}

install_rust_secure() {
    if command_exists cargo; then
        local rust_version
        rust_version=$(rustc --version 2>/dev/null || echo "unknown")
        log_success "Rust already installed: $rust_version"
        return 0
    fi

    log_info "Installing Rust toolchain securely..."

    # Create temporary file for installer
    local temp_installer
    temp_installer=$(mktemp) || handle_error $ERR_INSTALL_FAILED \
        "Failed to create temporary file" \
        "Check disk space and permissions in /tmp"

    # Ensure cleanup on exit
    trap "rm -f '$temp_installer'" EXIT

    # Download rustup installer with timeout and security flags
    log_info "Downloading Rust installer..."
    if ! timeout "$TIMEOUT_SECONDS" curl \
        --proto '=https' \
        --tlsv1.2 \
        --silent \
        --show-error \
        --fail \
        --location \
        --output "$temp_installer" \
        https://sh.rustup.rs; then

        handle_error $ERR_NETWORK_TIMEOUT \
            "Failed to download Rust installer" \
            "Check your internet connection and try again"
    fi

    # Verify the installer checksum
    if ! verify_rustup_checksum "$temp_installer"; then
        handle_error $ERR_VERIFICATION_FAILED \
            "Rust installer verification failed" \
            "The download may be corrupted or tampered with. Please try again"
    fi

    # Execute the verified installer
    log_info "Running Rust installer..."
    if ! bash "$temp_installer" -y --no-modify-path 2>&1 | tee -a "$LOG_FILE"; then
        handle_error $ERR_INSTALL_FAILED \
            "Rust installation failed" \
            "Check the log file for details: $LOG_FILE"
    fi

    # Source cargo environment immediately
    if [[ -f "$HOME/.cargo/env" ]]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
        log_success "Rust toolchain installed successfully"
    else
        handle_error $ERR_INSTALL_FAILED \
            "Rust environment file not found" \
            "Installation may be incomplete. Visit https://rustup.rs for manual installation"
    fi

    # Remove trap since we're done
    trap - EXIT
    rm -f "$temp_installer"

    return 0
}

# ============================================================================
# Build Tools Detection (for source builds)
# ============================================================================

check_build_tools() {
    log_info "Checking for build tools..."

    # Check for C compiler/linker (required for Rust linking)
    local has_cc=false
    local cc_command=""

    if command_exists gcc; then
        has_cc=true
        cc_command="gcc"
    elif command_exists clang; then
        has_cc=true
        cc_command="clang"
    elif command_exists cc; then
        has_cc=true
        cc_command="cc"
    fi

    if [[ "$has_cc" == "true" ]]; then
        log_success "C compiler found: $cc_command"
        return 0
    fi

    # No C compiler found - show platform-specific instructions
    log_error "C compiler/linker not found (required for building from source)"
    echo ""
    echo -e "${YELLOW}═══════════════════════════════════════════${NC}"
    echo -e "${YELLOW}⚠️  Missing Build Tools${NC}"
    echo -e "${YELLOW}═══════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BLUE}Rust requires a C linker for the final compilation step.${NC}"
    echo ""
    echo -e "${BLUE}Install build tools for your platform:${NC}"
    echo ""

    case "$OS_TYPE" in
        macos)
            echo -e "  ${GREEN}xcode-select --install${NC}"
            echo ""
            echo -e "  (This installs Apple's command-line developer tools)"
            ;;
        ubuntu|debian)
            echo -e "  ${GREEN}sudo apt-get update${NC}"
            echo -e "  ${GREEN}sudo apt-get install -y build-essential${NC}"
            echo ""
            echo -e "  (This installs gcc, g++, make, and other essential tools)"
            ;;
        fedora|rhel|centos|rocky|almalinux)
            if command_exists dnf; then
                echo -e "  ${GREEN}sudo dnf install -y gcc gcc-c++ make${NC}"
            else
                echo -e "  ${GREEN}sudo yum install -y gcc gcc-c++ make${NC}"
            fi
            echo ""
            echo -e "  (This installs the C/C++ compiler and build tools)"
            ;;
        arch|manjaro|endeavouros)
            echo -e "  ${GREEN}sudo pacman -S base-devel${NC}"
            echo ""
            echo -e "  (This installs essential build tools)"
            ;;
        alpine)
            echo -e "  ${GREEN}sudo apk add build-base${NC}"
            echo ""
            echo -e "  (This installs essential build tools)"
            ;;
        *)
            echo -e "  ${YELLOW}Install gcc or clang using your package manager${NC}"
            echo ""
            ;;
    esac

    echo -e "${YELLOW}After installing, run this script again.${NC}"
    echo -e "${YELLOW}═══════════════════════════════════════════${NC}"
    echo ""

    handle_error $ERR_DEPENDENCY_MISSING \
        "Build tools not installed" \
        "Install build tools (see above) then retry"
}

# ============================================================================
# Build Dependencies Installation (mold linker and SSL libraries)
# ============================================================================

install_build_dependencies() {
    log_info "Installing build dependencies (mold linker, OpenSSL)..."

    case "$OS_TYPE" in
        ubuntu|debian)
            # Check if mold is already installed
            if command_exists mold; then
                log_success "mold linker already installed"
            else
                log_info "Installing mold linker..."
                if sudo apt-get update && sudo apt-get install -y mold; then
                    log_success "mold linker installed successfully"
                else
                    log_warning "Failed to install mold, build may fail"
                fi
            fi

            # Check if libssl-dev is already installed
            if dpkg -l | grep -q libssl-dev; then
                log_success "libssl-dev already installed"
            else
                log_info "Installing libssl-dev and pkg-config..."
                if sudo apt-get install -y libssl-dev pkg-config; then
                    log_success "SSL development libraries installed successfully"
                else
                    log_warning "Failed to install libssl-dev, build may fail"
                fi
            fi
            ;;

        macos)
            # macOS uses the default linker, no mold needed
            log_info "Using default macOS linker (mold not needed)"

            # Check for OpenSSL (usually installed via Homebrew)
            if ! brew list openssl &>/dev/null; then
                log_info "Installing OpenSSL..."
                if brew install openssl; then
                    log_success "OpenSSL installed successfully"
                else
                    log_warning "Failed to install OpenSSL, build may fail"
                fi
            else
                log_success "OpenSSL already installed"
            fi
            ;;

        fedora|rhel|centos|rocky|almalinux)
            # Install mold if available
            if command_exists mold; then
                log_success "mold linker already installed"
            else
                log_info "Installing mold linker..."
                if command_exists dnf; then
                    if sudo dnf install -y mold; then
                        log_success "mold linker installed successfully"
                    else
                        log_warning "mold not available in repos, build may be slower"
                    fi
                else
                    log_warning "mold not available, build may be slower"
                fi
            fi

            # Install OpenSSL development libraries
            log_info "Installing OpenSSL development libraries..."
            if command_exists dnf; then
                sudo dnf install -y openssl-devel pkg-config
            else
                sudo yum install -y openssl-devel pkg-config
            fi
            log_success "SSL development libraries installed successfully"
            ;;

        arch|manjaro|endeavouros)
            # Install mold
            if command_exists mold; then
                log_success "mold linker already installed"
            else
                log_info "Installing mold linker..."
                if sudo pacman -S --noconfirm mold; then
                    log_success "mold linker installed successfully"
                else
                    log_warning "Failed to install mold, build may fail"
                fi
            fi

            # Install OpenSSL
            log_info "Installing OpenSSL..."
            sudo pacman -S --noconfirm openssl pkg-config
            log_success "SSL development libraries installed successfully"
            ;;

        alpine)
            # Alpine uses musl, mold may not be available
            log_info "Installing build dependencies for Alpine..."
            if sudo apk add mold 2>/dev/null; then
                log_success "mold linker installed successfully"
            else
                log_warning "mold not available for Alpine, using default linker"
            fi

            sudo apk add openssl-dev pkgconf
            log_success "SSL development libraries installed successfully"
            ;;

        *)
            log_warning "Unknown OS type: $OS_TYPE"
            log_warning "You may need to manually install: mold, libssl-dev, pkg-config"
            ;;
    esac

    return 0
}

# ============================================================================
# Path Configuration
# ============================================================================

configure_path_safely() {
    local cargo_bin_path="$HOME/.cargo/bin"

    log_info "Configuring PATH in $SHELL_CONFIG..."

    # Create shell config if it doesn't exist
    if [[ ! -f "$SHELL_CONFIG" ]]; then
        touch "$SHELL_CONFIG" || handle_error $ERR_PATH_CONFIG \
            "Failed to create shell configuration file" \
            "Check permissions for $SHELL_CONFIG"
        log_info "Created: $SHELL_CONFIG"
    fi

    # Check if PATH is already configured
    if grep -q "$cargo_bin_path" "$SHELL_CONFIG" 2>/dev/null; then
        log_success "PATH already configured in $(basename "$SHELL_CONFIG")"
        return 0
    fi

    # Add PATH configuration based on shell type
    local path_line
    case "$SHELL_TYPE" in
        fish)
            path_line="set -gx PATH \$PATH $cargo_bin_path"
            ;;
        *)
            path_line="export PATH=\"\$PATH:$cargo_bin_path\""
            ;;
    esac

    # Add to shell config
    {
        echo ""
        echo "# Added by VM installer v$SCRIPT_VERSION"
        echo "$path_line"
    } >> "$SHELL_CONFIG"

    log_success "PATH updated in $(basename "$SHELL_CONFIG")"
    log_warning "Restart your terminal or run: source $SHELL_CONFIG"

    return 0
}

install_shell_completion() {
    local vm_binary=""
    local completion_path=""
    local source_line=""

    log_info "Installing shell completion for $SHELL_TYPE..."

    if [[ -x "$HOME/.local/bin/vm" ]]; then
        vm_binary="$HOME/.local/bin/vm"
    elif [[ -x "$HOME/.cargo/bin/vm" ]]; then
        vm_binary="$HOME/.cargo/bin/vm"
    elif command_exists vm; then
        vm_binary="$(command -v vm)"
    fi

    if [[ -z "$vm_binary" ]]; then
        log_warning "Skipping shell completion installation: vm binary not found"
        return 0
    fi

    case "$SHELL_TYPE" in
        bash)
            completion_path="$HOME/.vm-completion.bash"
            source_line="source ~/.vm-completion.bash"
            ;;
        zsh)
            completion_path="$HOME/.vm-completion.zsh"
            source_line="source ~/.vm-completion.zsh"
            ;;
        fish)
            completion_path="$HOME/.config/fish/completions/vm.fish"
            ;;
        *)
            log_warning "Skipping shell completion installation for unsupported shell: $SHELL_TYPE"
            return 0
            ;;
    esac

    mkdir -p "$(dirname "$completion_path")" || {
        log_warning "Failed to create completion directory for $completion_path"
        return 0
    }

    if ! "$vm_binary" internal-completion "$SHELL_TYPE" > "$completion_path"; then
        log_warning "Failed to generate shell completion for $SHELL_TYPE"
        return 0
    fi

    if [[ "$SHELL_TYPE" == "zsh" ]] && ! grep -Fq '${functions[compdef]+x}' "$completion_path" 2>/dev/null; then
        local completion_tmp
        completion_tmp=$(mktemp) || {
            log_warning "Failed to create temporary zsh completion file"
            return 0
        }
        {
            echo "# Ensure compdef is available when this file is sourced directly from .zshrc."
            echo 'if [[ -n ${ZSH_VERSION:-} && -z ${functions[compdef]+x} ]]; then'
            echo "  autoload -Uz compinit"
            echo "  compinit -i"
            echo "fi"
            echo ""
            cat "$completion_path"
        } > "$completion_tmp" && mv "$completion_tmp" "$completion_path"
        rm -f "$completion_tmp"
    fi

    if [[ -n "$source_line" ]]; then
        if [[ ! -f "$SHELL_CONFIG" ]]; then
            touch "$SHELL_CONFIG" || {
                log_warning "Failed to update $SHELL_CONFIG with completion source line"
                return 0
            }
        fi

        if ! grep -Fq "$source_line" "$SHELL_CONFIG" 2>/dev/null; then
            {
                echo ""
                echo "# Added by VM installer v$SCRIPT_VERSION"
                echo "$source_line"
            } >> "$SHELL_CONFIG"
        fi
    fi

    log_success "Shell completion installed at $completion_path"
    return 0
}

# ============================================================================
# VM Installation Functions
# ============================================================================

build_standalone_pkg_server() {
    log_info "Building standalone package server..."

    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

    cd "$script_dir/rust" || handle_error $ERR_CARGO_BUILD \
        "Failed to navigate to rust directory" \
        "Ensure the script is run from the project root"

    if ! timeout "$CARGO_TIMEOUT_SECONDS" cargo build --release --features standalone-binary -p vm-package-server 2>&1 | tee -a "$LOG_FILE"; then
        handle_error $ERR_CARGO_BUILD \
            "Failed to build standalone package server" \
            "Check the build log in $LOG_FILE"
    fi

    log_success "Standalone package server built successfully"

    # Install the standalone binary
    local pkg_server_bin="$script_dir/rust/target/release/pkg-server"
    if [[ ! -f "$pkg_server_bin" ]]; then
        handle_error $ERR_INSTALL_FAILED \
            "Built binary not found at expected location" \
            "Check if the build completed successfully"
    fi

    # Determine install directory
    local install_dir
    if [[ -w "/usr/local/bin" ]]; then
        install_dir="/usr/local/bin"
    elif [[ -d "$HOME/.local/bin" ]] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
        install_dir="$HOME/.local/bin"
    else
        handle_error $ERR_PERMISSION_DENIED \
            "No writable install directory found" \
            "Create ~/.local/bin or run with sudo"
    fi

    log_info "Installing pkg-server to $install_dir..."
    cp "$pkg_server_bin" "$install_dir/pkg-server" || handle_error $ERR_INSTALL_FAILED \
        "Failed to copy pkg-server binary" \
        "Check permissions for $install_dir"

    chmod +x "$install_dir/pkg-server" || handle_error $ERR_PERMISSION_DENIED \
        "Failed to make pkg-server executable" \
        "Check file permissions"

    log_success "pkg-server installed to $install_dir/pkg-server"

    cd "$script_dir" || true
    return 0
}

install_vm_tool() {
    log_info "Installing VM tool..."

    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local rust_dir="$script_dir/rust"

    if [[ ! -d "$rust_dir" ]]; then
        handle_error $ERR_INSTALL_FAILED \
            "Rust workspace not found" \
            "Ensure you're running the script from the project directory"
    fi

    # Run the Rust installer
    echo "📦 Installing VM tool from source..."
    echo "⏱️  This may take 2-3 minutes..."

    # Capture output to both log and temp file for error reporting
    local installer_output
    installer_output=$(mktemp)
    trap "rm -f '$installer_output'" EXIT

    # Store current directory and change to rust directory
    # This aligns the script with the manual workaround
    local current_dir
    current_dir=$(pwd)
    cd "$rust_dir" || handle_error $ERR_INSTALL_FAILED "Could not change to rust directory"

    local cargo_failed=false
    if ! timeout "$CARGO_TIMEOUT_SECONDS" cargo run \
        --package vm-installer \
        -- "${INSTALLER_ARGS[@]+"${INSTALLER_ARGS[@]}"}" 2>&1 | tee -a "$LOG_FILE" "$installer_output"; then
        cargo_failed=true
    fi

    # Change back to original directory
    cd "$current_dir" || log_warning "Could not change back to original directory"

    if [[ "$cargo_failed" == "true" ]]; then
        echo "❌ Installation failed"
        echo ""
        echo "Common fixes:"
        echo "  • Ensure Rust is up to date: rustup update"
        echo "  • Check internet connection"
        echo "  • Try: cargo install goobits-vm --locked"

        # Extract last meaningful error from output
        local error_detail
        error_detail=$(grep -E "^(Error|error:|❌)" "$installer_output" | tail -5 | tr '\n' ' ' || echo "")

        if [[ -z "$error_detail" ]]; then
            error_detail="Build completed but installer failed during setup"
        fi

        handle_error $ERR_INSTALL_FAILED \
            "VM installer failed: $error_detail" \
            "Check the full log at $LOG_FILE or run: cd rust && cargo run --package vm-installer"
    fi

    rm -f "$installer_output"
    trap - EXIT

    echo "✅ VM tool installed successfully"
    return 0
}

# ============================================================================
# Installation Verification (Phase 4)
# ============================================================================

verify_installation() {
    local checks_passed=0
    local checks_total=0
    local has_errors=false

    echo ""
    log_info "Running installation verification..."
    echo ""

    # Source cargo env to ensure PATH is updated
    if [[ -f "$HOME/.cargo/env" ]]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi

    # Check 1: VM binary exists
    ((checks_total++))
    if command_exists vm; then
        log_success "VM binary found in PATH"
        ((checks_passed++))
    else
        log_error "VM binary not found in PATH"
        has_errors=true
    fi

    # Check 2: VM binary is executable
    ((checks_total++))
    if [[ -x "$(command -v vm 2>/dev/null)" ]]; then
        log_success "VM binary is executable"
        ((checks_passed++))
    else
        log_error "VM binary not executable"
        has_errors=true
    fi

    # Check 3: VM responds to version
    ((checks_total++))
    if timeout 10 vm --version &>/dev/null; then
        local vm_version
        vm_version=$(vm --version 2>/dev/null | head -1)
        log_success "VM responds correctly: $vm_version"
        ((checks_passed++))
    else
        log_error "VM doesn't respond to --version"
        has_errors=true
    fi

    # Check 4: Cargo bin in PATH
    ((checks_total++))
    if echo "$PATH" | grep -q ".cargo/bin"; then
        log_success "Cargo bin directory in PATH"
        ((checks_passed++))
    else
        log_warning "Cargo bin not in PATH (will be added on next shell restart)"
        ((checks_passed++))  # Not a critical error
    fi

    # Check 5: Installation mode
    ((checks_total++))
    log_success "Built from source"
    ((checks_passed++))

    # Report results
    echo ""
    if [[ $checks_passed -eq $checks_total ]]; then
        log_success "All verification checks passed ($checks_passed/$checks_total)"
        return 0
    elif [[ "$has_errors" == "true" ]]; then
        log_error "Some critical checks failed ($checks_passed/$checks_total)"
        return 1
    else
        log_warning "Some non-critical checks failed ($checks_passed/$checks_total)"
        return 0
    fi
}

# ============================================================================
# Argument Parsing
# ============================================================================

parse_arguments() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --build-from-source)
                log_info "Building from source (default mode)"
                ;;
            --version)
                local requested_version="${2:-}"
                if [[ -z "$requested_version" ]] || [[ "$requested_version" == --* ]]; then
                    echo "error: --version requires an argument" >&2
                    exit $ERR_INSTALL_FAILED
                fi
                echo "error: versioned installs are not supported by this source installer." >&2
                echo "use 'cargo install goobits-vm --version ${requested_version#v} --locked' or check out tag '$requested_version' and rerun ./install.sh." >&2
                exit $ERR_INSTALL_FAILED
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            -v)
                echo "VM Installer v$SCRIPT_VERSION"
                exit 0
                ;;
            *)
                INSTALLER_ARGS+=("$1")
                ;;
        esac
        shift
    done
}

show_help() {
    cat << EOF
VM Infrastructure Installation Script v$SCRIPT_VERSION

Usage:
  $SCRIPT_NAME [OPTIONS]

Options:
  --build-from-source    Legacy alias; source install is the default
  --help, -h             Show this help message
  -v                     Show installer version information

Environment Variables:
  CARGO_HOME             Override installation directory (default: ~/.cargo)

Examples:
  # Install from source
  ./$SCRIPT_NAME

  # Legacy source-install alias
  ./$SCRIPT_NAME --build-from-source

For more information, visit: $REPO_URL
EOF
}

# ============================================================================
# Main Installation Logic
# ============================================================================

main() {
    # Initialize log file
    echo "═══════════════════════════════════════════" > "$LOG_FILE"
    echo "VM Installation Log - $(date '+%Y-%m-%d %H:%M:%S')" >> "$LOG_FILE"
    echo "Version: $SCRIPT_VERSION" >> "$LOG_FILE"
    echo "Mode: source" >> "$LOG_FILE"
    echo "═══════════════════════════════════════════" >> "$LOG_FILE"

    echo ""
    echo -e "${GREEN}${LOG_PREFIX} v$SCRIPT_VERSION${NC}"
    echo -e "${BLUE}Installing from: source${NC}"
    echo ""

    # Step 1: Platform detection
    detect_platform || handle_error $ERR_PLATFORM_DETECT \
        "Platform detection failed" \
        "Please report this issue with your OS details"

    detect_shell_config
    echo ""

    # Step 2: Build from source
    log_info "Building from source..."

    # Install Rust if needed
    install_rust_secure || handle_error $ERR_INSTALL_FAILED \
        "Rust installation failed" \
        "Try installing Rust manually from https://rustup.rs"
    echo ""

    # Check for build tools (gcc/clang)
    check_build_tools
    echo ""

    # Install build dependencies (mold, OpenSSL)
    install_build_dependencies
    echo ""

    # Install VM tool from source
    install_vm_tool
    echo ""

    # Step 3: Configure PATH
    configure_path_safely
    echo ""

    # Step 4: Install shell completion
    install_shell_completion
    echo ""

    # Step 5: Verify installation
    if ! verify_installation; then
        log_warning "Installation completed with warnings"
        log_info "Please check the log file: $LOG_FILE"
    fi

    # Step 6: Success message
    echo ""
    echo -e "${GREEN}═══════════════════════════════════════════${NC}"
    echo -e "${GREEN}🎉 Installation completed successfully!${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════${NC}"
    echo ""

    # Show next steps
    echo -e "${BLUE}Next steps:${NC}"
    echo -e "  1. Restart your terminal or run: ${YELLOW}source $SHELL_CONFIG${NC}"
    echo -e "  2. Get started with: ${YELLOW}vm --help${NC}"
    echo ""
    echo -e "${BLUE}Documentation:${NC} $REPO_URL"
    echo -e "${BLUE}Support:${NC} ${REPO_URL}/issues"
    echo ""

    return 0
}

# ============================================================================
# Script Entry Point
# ============================================================================

# Validate script syntax before execution
if ! bash -n "$0" 2>/dev/null; then
    echo "❌ Script syntax validation failed" >&2
    exit 1
fi

# Check for required commands
for cmd in curl timeout mktemp; do
    if ! command_exists "$cmd"; then
        echo "❌ Required command '$cmd' not found" >&2
        echo "💡 Please install '$cmd' and try again" >&2
        exit $ERR_DEPENDENCY_MISSING
    fi
done

# Parse arguments
parse_arguments "$@"

# Run main installation
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main
fi
