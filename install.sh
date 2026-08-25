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
        echo -e "  Shell: ${SHELL:-unknown}"
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
# VM Installation Functions
# ============================================================================

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

    local source_target_dir="${CARGO_TARGET_DIR:-/tmp/vm-rust-target}"
    local cargo_failed=false
    if ! timeout "$CARGO_TIMEOUT_SECONDS" env CARGO_TARGET_DIR="$source_target_dir" cargo run \
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
        echo "  • Try: cd rust && cargo run --package vm-installer --"

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
# Argument Parsing
# ============================================================================

parse_arguments() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --version)
                local requested_version="${2:-}"
                if [[ -z "$requested_version" ]] || [[ "$requested_version" == --* ]]; then
                    echo "error: --version requires an argument" >&2
                    exit $ERR_INSTALL_FAILED
                fi
                echo "error: versioned installs are not supported by this source installer." >&2
                echo "check out tag '$requested_version' and rerun ./install.sh." >&2
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
  --help, -h             Show this help message
  -v                     Show installer version information

Environment Variables:
  CARGO_HOME             Override installation directory (default: ~/.cargo)

Examples:
  # Install from source
  ./$SCRIPT_NAME

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

    # Step 3: Success message
    echo ""
    echo -e "${GREEN}═══════════════════════════════════════════${NC}"
    echo -e "${GREEN}🎉 Installation completed successfully!${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════${NC}"
    echo ""

    # Show next steps
    echo -e "${BLUE}Next step:${NC} ${YELLOW}vm --help${NC}"
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

# Parse arguments
parse_arguments "$@"

# Check for required commands after parsing so --help and -v work on fresh systems.
for cmd in curl timeout mktemp; do
    if ! command_exists "$cmd"; then
        echo "❌ Required command '$cmd' not found" >&2
        echo "💡 Please install '$cmd' and try again" >&2
        exit $ERR_DEPENDENCY_MISSING
    fi
done

# Run main installation
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main
fi
