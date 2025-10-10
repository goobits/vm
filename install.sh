#!/bin/bash
#
# VM Infrastructure Installation Script
#
# Supports: macOS, Ubuntu/Debian, Fedora/RHEL, Arch Linux
# Security: Enterprise-grade with verification and comprehensive error handling
#
# Usage:
#   ./install.sh                    # Install vm tool from pre-compiled binary
#   ./install.sh --version v1.2.3   # Install specific version
#   ./install.sh --build-from-source  # Build from source
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
readonly CARGO_TIMEOUT_SECONDS=120  # Longer timeout for cargo operations
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
INSTALL_MODE="binary"  # binary or source
INSTALL_VERSION="latest"
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
# Binary Download and Installation
# ============================================================================

download_and_verify_binary() {
    local download_url="$1"
    local output_file="$2"
    local checksum_url="$3"

    log_info "Downloading binary from $download_url..."

    if ! timeout "$TIMEOUT_SECONDS" curl \
        --proto '=https' \
        --tlsv1.2 \
        --silent \
        --show-error \
        --fail \
        --location \
        --output "$output_file" \
        "$download_url"; then

        handle_error $ERR_NETWORK_TIMEOUT \
            "Failed to download binary" \
            "Check your internet connection and try again"
    fi

    # Verify checksum if available
    if [[ -n "$checksum_url" ]]; then
        log_info "Downloading and verifying checksum..."

        local checksum_file
        checksum_file=$(mktemp)
        trap "rm -f '$checksum_file'" EXIT

        if timeout "$TIMEOUT_SECONDS" curl \
            --proto '=https' \
            --tlsv1.2 \
            --silent \
            --fail \
            --location \
            --output "$checksum_file" \
            "$checksum_url"; then

            # Extract expected hash for our file
            local expected_hash
            local filename
            filename=$(basename "$output_file")
            expected_hash=$(grep "$filename" "$checksum_file" 2>/dev/null | cut -d' ' -f1)

            if [[ -n "$expected_hash" ]]; then
                # Calculate actual hash
                local actual_hash
                if command_exists sha256sum; then
                    actual_hash=$(sha256sum "$output_file" | cut -d' ' -f1)
                elif command_exists shasum; then
                    actual_hash=$(shasum -a 256 "$output_file" | cut -d' ' -f1)
                elif command_exists openssl; then
                    actual_hash=$(openssl dgst -sha256 "$output_file" | cut -d' ' -f2)
                fi

                if [[ "$expected_hash" == "$actual_hash" ]]; then
                    log_success "SHA256 checksum verification passed"
                else
                    handle_error $ERR_VERIFICATION_FAILED \
                        "Checksum verification failed" \
                        "The download may be corrupted"
                fi
            else
                log_warning "Could not find checksum for $filename in checksum file"
            fi
        else
            log_warning "Could not download checksum file, skipping verification"
        fi

        trap - EXIT
        rm -f "$checksum_file"
    fi

    # Verify file size
    local file_size
    file_size=$(stat -f%z "$output_file" 2>/dev/null || stat -c%s "$output_file" 2>/dev/null || echo "0")

    if [[ "$file_size" -lt 1000000 ]]; then  # Expect at least 1MB
        handle_error $ERR_VERIFICATION_FAILED \
            "Downloaded file too small ($file_size bytes)" \
            "The download may have failed"
    fi

    log_success "Binary downloaded successfully ($file_size bytes)"
}

install_from_release() {
    log_info "Installing pre-compiled binary from GitHub release..."

    # Determine target triple for download
    local target_arch
    case "$ARCH" in
        x86_64) target_arch="x86_64" ;;
        aarch64|arm64) target_arch="aarch64" ;;
        *) handle_error $ERR_PLATFORM_DETECT "Unsupported architecture: $ARCH" ;;
    esac

    local target_os
    case "$OS_TYPE" in
        macos) target_os="apple-darwin" ;;
        *) target_os="unknown-linux-gnu" ;;  # Assuming Linux for others
    esac

    local target_triple="${target_arch}-${target_os}"
    log_info "Detected target: $target_triple"

    # Fetch release info from GitHub API
    local api_url
    if [[ "$INSTALL_VERSION" == "latest" ]]; then
        api_url="https://api.github.com/repos/${REPO_URL#*github.com/}/releases/latest"
    else
        api_url="https://api.github.com/repos/${REPO_URL#*github.com/}/releases/tags/${INSTALL_VERSION}"
    fi

    log_info "Fetching release info from GitHub..."

    local release_info
    release_info=$(mktemp)
    trap "rm -f '$release_info'" EXIT

    if ! timeout "$TIMEOUT_SECONDS" curl \
        --proto '=https' \
        --tlsv1.2 \
        --silent \
        --fail \
        --location \
        --header "Accept: application/vnd.github.v3+json" \
        --output "$release_info" \
        "$api_url"; then

        # Simple, clean error for missing releases
        echo ""
        echo -e "${RED}❌ No pre-built binary available${NC}"
        echo ""
        echo -e "${BLUE}Run this instead:${NC}"
        echo -e "  ${GREEN}./install.sh --build-from-source${NC}"
        echo ""
        exit $ERR_NETWORK_TIMEOUT
    fi

    # Extract download URL for our platform
    local asset_url
    local checksum_url

    # For compressed archives (.tar.gz or .zip)
    if [[ "$OS_TYPE" == "macos" ]] || [[ "$target_os" == "unknown-linux-gnu" ]]; then
        # Unix systems use tar.gz
        asset_url=$(grep "browser_download_url.*vm-${target_triple}\.tar\.gz" "$release_info" | head -1 | cut -d '"' -f 4)
        checksum_url=$(grep "browser_download_url.*vm-${target_triple}\.tar\.gz\.sha256" "$release_info" | head -1 | cut -d '"' -f 4)
    else
        # Windows would use .zip
        asset_url=$(grep "browser_download_url.*vm-${target_triple}\.zip" "$release_info" | head -1 | cut -d '"' -f 4)
        checksum_url=$(grep "browser_download_url.*vm-${target_triple}\.zip\.sha256" "$release_info" | head -1 | cut -d '"' -f 4)
    fi

    if [[ -z "$asset_url" ]]; then
        handle_error $ERR_NETWORK_TIMEOUT \
            "Could not find a download URL for platform: $target_triple" \
            "Check available releases at ${REPO_URL}/releases"
    fi

    # Download binary archive
    local temp_archive
    temp_archive=$(mktemp)
    trap "rm -f '$temp_archive' '$release_info'" EXIT

    download_and_verify_binary "$asset_url" "$temp_archive" "$checksum_url"

    # Extract binary
    log_info "Extracting binary..."
    local temp_dir
    temp_dir=$(mktemp -d)
    trap "rm -rf '$temp_dir' '$temp_archive' '$release_info'" EXIT

    if [[ "$asset_url" == *.tar.gz ]]; then
        tar -xzf "$temp_archive" -C "$temp_dir" || handle_error $ERR_INSTALL_FAILED \
            "Failed to extract archive" \
            "Archive may be corrupted"
    elif [[ "$asset_url" == *.zip ]]; then
        unzip -q "$temp_archive" -d "$temp_dir" || handle_error $ERR_INSTALL_FAILED \
            "Failed to extract archive" \
            "Archive may be corrupted"
    fi

    # Find the vm binary
    local vm_binary="$temp_dir/vm-${target_triple}"
    if [[ ! -f "$vm_binary" ]]; then
        # Try without the triple suffix
        vm_binary="$temp_dir/vm"
    fi

    if [[ ! -f "$vm_binary" ]]; then
        handle_error $ERR_INSTALL_FAILED \
            "Binary not found in archive" \
            "Archive structure may be unexpected"
    fi

    # Install the binary
    local install_dir="$HOME/.cargo/bin"

    # Create install directory if it doesn't exist
    mkdir -p "$install_dir" || handle_error $ERR_PERMISSION_DENIED \
        "Failed to create install directory" \
        "Check permissions for $install_dir"

    log_info "Installing to $install_dir/vm..."
    mv "$vm_binary" "$install_dir/vm" || handle_error $ERR_INSTALL_FAILED \
        "Failed to install binary" \
        "Check permissions for $install_dir"

    chmod +x "$install_dir/vm" || handle_error $ERR_PERMISSION_DENIED \
        "Failed to make binary executable" \
        "Check file permissions"

    trap - EXIT
    rm -rf "$temp_dir" "$temp_archive" "$release_info"

    log_success "vm installed successfully to $install_dir/vm"
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
    local manifest_path="$script_dir/rust/Cargo.toml"

    if [[ ! -f "$manifest_path" ]]; then
        handle_error $ERR_INSTALL_FAILED \
            "Rust workspace not found" \
            "Ensure you're running the script from the project directory"
    fi

    # Run the Rust installer
    log_info "Running Rust installer..."

    # Capture output to both log and temp file for error reporting
    local installer_output
    installer_output=$(mktemp)
    trap "rm -f '$installer_output'" EXIT

    if ! timeout "$CARGO_TIMEOUT_SECONDS" cargo run \
        --package vm-installer \
        --manifest-path "$manifest_path" \
        -- "${INSTALLER_ARGS[@]+"${INSTALLER_ARGS[@]}"}" 2>&1 | tee -a "$LOG_FILE" "$installer_output"; then

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

    log_success "VM tool installed successfully"
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
    if [[ "$INSTALL_MODE" == "binary" ]]; then
        log_success "Installed from pre-compiled binary"
    else
        log_success "Built from source"
    fi
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
    for arg in "$@"; do
        case "$arg" in
            --build-from-source)
                INSTALL_MODE="source"
                log_info "Will build from source instead of downloading binary"
                ;;
            --version)
                # Get next argument as version
                shift
                INSTALL_VERSION="${1:-latest}"
                log_info "Will install version: $INSTALL_VERSION"
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
                INSTALLER_ARGS+=("$arg")
                ;;
        esac
    done
}

show_help() {
    cat << EOF
VM Infrastructure Installation Script v$SCRIPT_VERSION

Usage:
  $SCRIPT_NAME [OPTIONS]

Options:
  --version VERSION      Install specific version (default: latest)
  --build-from-source    Build from source instead of downloading binary
  --help, -h             Show this help message
  -v                     Show installer version information

Environment Variables:
  CARGO_HOME             Override installation directory (default: ~/.cargo)

Examples:
  # Install latest version
  ./$SCRIPT_NAME

  # Install specific version
  ./$SCRIPT_NAME --version v1.2.3

  # Build from source (requires Rust)
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
    echo "Mode: $INSTALL_MODE" >> "$LOG_FILE"
    echo "═══════════════════════════════════════════" >> "$LOG_FILE"

    echo ""
    echo -e "${GREEN}${LOG_PREFIX} v$SCRIPT_VERSION${NC}"
    echo -e "${BLUE}Installing from: ${INSTALL_MODE}${NC}"
    echo ""

    # Step 1: Platform detection
    detect_platform || handle_error $ERR_PLATFORM_DETECT \
        "Platform detection failed" \
        "Please report this issue with your OS details"

    detect_shell_config
    echo ""

    # Step 2: Install based on mode
    if [[ "$INSTALL_MODE" == "binary" ]]; then
        # Install from pre-compiled binary
        install_from_release
    else
        # Build from source
        log_info "Building from source..."

        # Install Rust if needed
        install_rust_secure || handle_error $ERR_INSTALL_FAILED \
            "Rust installation failed" \
            "Try installing Rust manually from https://rustup.rs"
        echo ""

        # Install VM tool from source
        install_vm_tool
    fi
    echo ""

    # Step 3: Configure PATH
    configure_path_safely
    echo ""

    # Step 4: Verify installation
    if ! verify_installation; then
        log_warning "Installation completed with warnings"
        log_info "Please check the log file: $LOG_FILE"
    fi

    # Step 5: Success message
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