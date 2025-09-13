#!/bin/bash
# Tart Provider Implementation for Apple Silicon Macs
# Supports macOS and Linux ARM64 VMs via Virtualization.framework

set -e

# Get the directory containing this script
TART_PROVIDER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_DIR="$(cd "$TART_PROVIDER_DIR/../.." && pwd)"

# Source utilities
source "$SCRIPT_DIR/shared/platform-utils.sh"
source "$SCRIPT_DIR/shared/logging-utils.sh"

# Tart installation check
check_tart_installed() {
    if ! command -v tart >/dev/null 2>&1; then
        echo "❌ Tart is not installed" >&2
        echo "" >&2
        echo "📦 To install Tart on macOS:" >&2
        echo "   brew install cirruslabs/cli/tart" >&2
        echo "" >&2
        echo "📖 Learn more: https://github.com/cirruslabs/tart" >&2
        return 1
    fi
    return 0
}

# Check if running on Apple Silicon
check_apple_silicon() {
    if [[ "$(uname -s)" != "Darwin" ]] || [[ "$(uname -m)" != "arm64" ]]; then
        echo "❌ Tart requires Apple Silicon Mac (M1/M2/M3/M4)" >&2
        echo "💡 Current system: $(uname -s) $(uname -m)" >&2
        return 1
    fi
    return 0
}

# Helper function to query config using vm-config binary directly
get_config() {
    local config="$1"
    local path="$2"
    local default="$3"
    # Use vm-config query directly on the merged config
    if [[ -n "${VM_CONFIG:-}" ]] && [[ -x "${VM_CONFIG}" ]]; then
        "$VM_CONFIG" query <(echo "$config") "$path" --raw --default "$default" 2>/dev/null || echo "$default"
    else
        # Fallback to yq
        local value
        value="$(echo "$config" | yq eval "$path" - 2>/dev/null)"
        if [[ -z "$value" || "$value" == "null" ]]; then
            echo "$default"
        else
            echo "$value"
        fi
    fi
}

# Get VM name from config
get_tart_vm_name() {
    local config="$1"
    local project_name=$(get_config "$config" ".project.name" "default")
    # Sanitize name for Tart (alphanumeric and hyphens only)
    project_name=$(echo "$project_name" | tr -cd '[:alnum:]-' | tr '[:upper:]' '[:lower:]')
    echo "vm-${project_name}"
}

# Detect guest OS from config or image
detect_guest_os() {
    local config="$1"
    
    # Check if OS field is set (new simple configuration)
    local os=$(get_config "$config" '.os' '')
    if [[ -n "$os" ]] && [[ "$os" != "auto" ]]; then
        case "$os" in
            macos)
                echo "macos"
                return
                ;;
            ubuntu|debian|alpine|linux)
                echo "linux"
                return
                ;;
        esac
    fi
    
    # Check if explicitly set in tart config
    local guest_os=$(get_config "$config" '.tart.guest_os' '')
    if [[ -n "$guest_os" ]]; then
        echo "$guest_os"
        return
    fi
    
    # Check if using a known image
    local image=$(get_config "$config" '.tart.image' '')
    case "$image" in
        *ubuntu*|*debian*|*linux*)
            echo "linux"
            ;;
        *monterey*|*ventura*|*sonoma*|*sequoia*|*macos*)
            echo "macos"
            ;;
        *)
            echo "linux"  # Default to Linux
            ;;
    esac
}

# Set up Tart environment with custom storage if specified
setup_tart_storage() {
    local config="$1"
    
    # Check for custom storage path in config
    local storage_path=$(get_config "$config" '.tart.storage_path' '')
    
    if [[ -n "$storage_path" ]] && [[ "$storage_path" != "null" ]]; then
        # Expand tilde if present
        storage_path="${storage_path/#\~/$HOME}"
        
        # Check if path exists
        if [[ ! -d "$storage_path" ]]; then
            echo "📁 Creating storage directory: $storage_path"
            mkdir -p "$storage_path" || {
                echo "❌ Failed to create storage directory: $storage_path" >&2
                return 1
            }
        fi
        
        # Verify write permissions
        if [[ ! -w "$storage_path" ]]; then
            echo "❌ No write permission for storage path: $storage_path" >&2
            return 1
        fi
        
        # Set TART_HOME environment variable
        export TART_HOME="$storage_path"
        echo "💾 Using custom storage: $storage_path"
        
        # Show available space
        local available_space=$(df -h "$storage_path" | awk 'NR==2 {print $4}')
        echo "   Available space: $available_space"
    fi
}

# Main Tart command wrapper implementation
tart_command_wrapper_impl() {
    local command="$1"
    local config="$2"
    local project_dir="$3"
    shift 3
    
    # Verify Tart is available
    if ! check_tart_installed || ! check_apple_silicon; then
        return 1
    fi
    
    # Set up custom storage if specified
    setup_tart_storage "$config"
    
    case "$command" in
        "create"|"up")
            tart_create "$config" "$project_dir" "$@"
            ;;
        "ssh")
            tart_ssh "$config" "$project_dir" "$@"
            ;;
        "start"|"resume")
            tart_start "$config" "$project_dir" "$@"
            ;;
        "stop"|"halt")
            tart_stop "$config" "$project_dir" "$@"
            ;;
        "destroy")
            tart_destroy "$config" "$project_dir" "$@"
            ;;
        "status")
            tart_status "$config" "$project_dir" "$@"
            ;;
        "exec")
            tart_exec "$config" "$project_dir" "$@"
            ;;
        "provision")
            tart_provision "$config" "$project_dir" "$@"
            ;;
        "restart"|"reload")
            tart_restart "$config" "$project_dir" "$@"
            ;;
        "logs")
            tart_logs "$config" "$project_dir" "$@"
            ;;
        "kill")
            tart_stop "$config" "$project_dir" "$@"
            ;;
        *)
            echo "❌ Command '$command' not implemented for Tart provider" >&2
            return 1
            ;;
    esac
}

# Create/start VM
tart_create() {
    local config="$1"
    local project_dir="$2"
    
    local vm_name=$(get_tart_vm_name "$config")
    local guest_os=$(detect_guest_os "$config")
    
    # Check if VM already exists
    if tart list 2>/dev/null | grep -q "^${vm_name}"; then
        echo "⚠️  VM '$vm_name' already exists"
        echo -n "Recreate it? This will destroy the existing VM. (y/N): "
        read -r response
        if [[ "$response" =~ ^[yY] ]]; then
            echo "🗑️  Destroying existing VM..."
            tart delete "$vm_name" 2>/dev/null || true
        else
            echo "Starting existing VM..."
            tart run "$vm_name" --no-graphics &
            sleep 3
            echo "✅ VM started"
            return 0
        fi
    fi
    
    # Get configuration values
    local image=$(get_config "$config" '.tart.image' '')
    local cpu=$(get_config "$config" '.vm.cpus // 4')
    local memory=$(get_config "$config" '.vm.memory // 4096')
    local disk_size=$(get_config "$config" '.tart.disk_size // 50')
    
    # Convert memory from MB to GB
    local memory_gb=$((memory / 1024))
    if [[ $memory_gb -lt 1 ]]; then
        memory_gb=1
    fi
    
    echo "🍎 Creating $guest_os VM with Tart..."
    echo "   Name: $vm_name"
    echo "   CPUs: $cpu"
    echo "   Memory: ${memory_gb}GB"
    echo "   Disk: ${disk_size}GB"
    
    # Create VM based on guest OS
    case "$guest_os" in
        "macos")
            create_macos_vm "$vm_name" "$image" "$cpu" "$memory_gb" "$disk_size" "$project_dir" "$config"
            ;;
        "linux")
            create_linux_vm "$vm_name" "$image" "$cpu" "$memory_gb" "$disk_size" "$project_dir" "$config"
            ;;
        *)
            echo "❌ Unsupported guest OS: $guest_os" >&2
            return 1
            ;;
    esac
}

# Create macOS VM
create_macos_vm() {
    local vm_name="$1"
    local image="$2"
    local cpu="$3"
    local memory="$4"
    local disk_size="$5"
    local project_dir="$6"
    local config="$7"
    
    if [[ -z "$image" ]]; then
        # Default macOS image
        image="ghcr.io/cirruslabs/macos-sonoma-base:latest"
    fi
    
    echo "📦 Pulling macOS image: $image"
    if ! tart clone "$image" "$vm_name"; then
        echo "❌ Failed to pull macOS image" >&2
        echo "💡 Try a different image or check your internet connection" >&2
        return 1
    fi
    
    # Configure VM
    echo "⚙️  Configuring VM..."
    tart set "$vm_name" --cpu "$cpu" --memory "$memory"
    
    # Note: Disk resizing is done during clone, not separately
    
    # Start VM
    echo "🚀 Starting macOS VM..."
    tart run "$vm_name" --no-graphics &
    
    # Wait for VM to boot
    echo "⏳ Waiting for VM to boot (this may take a minute)..."
    sleep 10
    
    echo ""
    echo "✅ macOS VM created successfully!"
    echo "💡 Use 'vm ssh' to connect"
    echo "   Default user: admin, password: admin"
}

# Create Linux VM
create_linux_vm() {
    local vm_name="$1"
    local image="$2"
    local cpu="$3"
    local memory="$4"
    local disk_size="$5"
    local project_dir="$6"
    local config="$7"
    
    if [[ -z "$image" ]]; then
        # Default to Ubuntu
        image="ghcr.io/cirruslabs/ubuntu:latest"
    fi
    
    echo "📦 Pulling Linux image: $image"
    if ! tart clone "$image" "$vm_name"; then
        echo "❌ Failed to pull Linux image" >&2
        echo "💡 Available images:" >&2
        echo "   - ghcr.io/cirruslabs/ubuntu:latest" >&2
        echo "   - ghcr.io/cirruslabs/debian:latest" >&2
        return 1
    fi
    
    # Configure VM
    echo "⚙️  Configuring VM..."
    tart set "$vm_name" --cpu "$cpu" --memory "$memory"
    
    # Enable Rosetta for x86 emulation if requested
    local enable_rosetta=$(get_config "$config" '.tart.rosetta // true')
    if [[ "$enable_rosetta" == "true" ]]; then
        echo "🔄 Enabling Rosetta 2 for x86 emulation..."
        tart set "$vm_name" --rosetta
    fi
    
    # Start VM
    echo "🚀 Starting Linux VM..."
    tart run "$vm_name" --no-graphics &
    
    # Wait for VM to boot
    echo "⏳ Waiting for VM to boot..."
    sleep 8
    
    echo ""
    echo "✅ Linux VM created successfully!"
    echo "💡 Use 'vm ssh' to connect"
}

# SSH into VM
tart_ssh() {
    local config="$1"
    local project_dir="$2"
    local vm_name=$(get_tart_vm_name "$config")
    
    # Set up custom storage if specified
    setup_tart_storage "$config" >/dev/null 2>&1
    
    # Check if VM exists
    if ! tart list 2>/dev/null | grep -q "^${vm_name}"; then
        echo "❌ VM '$vm_name' does not exist" >&2
        echo "💡 Run 'vm create' first" >&2
        return 1
    fi
    
    # Get VM IP (with retry logic)
    local vm_ip=""
    local retries=10
    
    echo "🔍 Getting VM IP address..."
    for ((i=1; i<=retries; i++)); do
        vm_ip=$(tart ip "$vm_name" 2>/dev/null || true)
        if [[ -n "$vm_ip" ]]; then
            break
        fi
        if [[ $i -eq $retries ]]; then
            echo "❌ Could not get VM IP address" >&2
            echo "💡 Make sure the VM is running: vm start" >&2
            return 1
        fi
        sleep 2
    done
    
    local guest_os=$(detect_guest_os "$config")
    local ssh_user=$(get_config "$config" '.tart.ssh_user // empty')
    
    # Default SSH users
    if [[ -z "$ssh_user" ]]; then
        case "$guest_os" in
            "macos")
                ssh_user="admin"
                ;;
            "linux")
                ssh_user="ubuntu"
                ;;
        esac
    fi
    
    # Calculate relative path for initial directory
    local relative_path="."
    if [[ -n "${CURRENT_DIR:-}" ]] && [[ -n "$project_dir" ]]; then
        relative_path=$(portable_relative_path "$project_dir" "$CURRENT_DIR" 2>/dev/null || echo ".")
    fi
    
    # Get workspace path
    local workspace_path=$(get_config "$config" '.project.workspace_path // "/workspace"')
    
    echo "🔗 Connecting to $vm_name at $vm_ip..."
    
    # Build SSH command with initial directory if needed
    local ssh_cmd="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR"
    
    if [[ "$relative_path" != "." ]]; then
        # SSH with initial directory change
        local target_dir="${workspace_path}/${relative_path}"
        $ssh_cmd -t "${ssh_user}@${vm_ip}" "cd ${target_dir} 2>/dev/null; exec \$SHELL -l"
    else
        # Simple SSH
        $ssh_cmd "${ssh_user}@${vm_ip}"
    fi
}

# Start VM
tart_start() {
    local config="$1"
    local vm_name=$(get_tart_vm_name "$config")
    
    # Set up custom storage if specified
    setup_tart_storage "$config" >/dev/null 2>&1
    
    if ! tart list 2>/dev/null | grep -q "^${vm_name}"; then
        echo "❌ VM '$vm_name' does not exist" >&2
        return 1
    fi
    
    echo "▶️  Starting VM '$vm_name'..."
    tart run "$vm_name" --no-graphics &
    sleep 3
    echo "✅ VM started"
}

# Stop VM
tart_stop() {
    local config="$1"
    local vm_name=$(get_tart_vm_name "$config")
    
    # Set up custom storage if specified
    setup_tart_storage "$config" >/dev/null 2>&1
    
    if ! tart list 2>/dev/null | grep -q "^${vm_name}"; then
        echo "❌ VM '$vm_name' does not exist" >&2
        return 1
    fi
    
    echo "⏸️  Stopping VM '$vm_name'..."
    tart stop "$vm_name" 2>/dev/null || true
    echo "✅ VM stopped"
}

# Restart VM
tart_restart() {
    local config="$1"
    local project_dir="$2"
    
    tart_stop "$config" "$project_dir"
    sleep 2
    tart_start "$config" "$project_dir"
}

# Destroy VM
tart_destroy() {
    local config="$1"
    local vm_name=$(get_tart_vm_name "$config")
    
    # Set up custom storage if specified
    setup_tart_storage "$config" >/dev/null 2>&1
    
    if ! tart list 2>/dev/null | grep -q "^${vm_name}"; then
        echo "⚠️  VM '$vm_name' does not exist"
        return 0
    fi
    
    echo "🗑️  Destroying VM '$vm_name'..."
    
    # Stop if running
    tart stop "$vm_name" 2>/dev/null || true
    
    # Delete VM
    tart delete "$vm_name"
    
    echo "✅ VM destroyed"
}

# Status check
tart_status() {
    local config="$1"
    local vm_name=$(get_tart_vm_name "$config")
    
    # Set up custom storage if specified
    setup_tart_storage "$config" >/dev/null 2>&1
    
    echo "📊 Tart VM Status:"
    echo ""
    
    # Check if VM exists
    if ! tart list 2>/dev/null | grep -q "^${vm_name}"; then
        echo "❌ VM '$vm_name' does not exist"
        echo "💡 Run 'vm create' to create a new VM"
        return 1
    fi
    
    # Get VM info
    echo "VM Name: $vm_name"
    
    # Check if running and get IP
    local vm_ip=$(tart ip "$vm_name" 2>/dev/null || echo "")
    if [[ -n "$vm_ip" ]]; then
        echo "Status: ✅ Running"
        echo "IP Address: $vm_ip"
    else
        echo "Status: ⏸️  Stopped"
    fi
    
    # Show VM details from list
    echo ""
    echo "Details:"
    tart list 2>/dev/null | grep "^${vm_name}" || echo "No details available"
}

# Execute command in VM
tart_exec() {
    local config="$1"
    local project_dir="$2"
    shift 2
    
    local vm_name=$(get_tart_vm_name "$config")
    
    # Get VM IP
    local vm_ip=$(tart ip "$vm_name" 2>/dev/null)
    
    if [[ -z "$vm_ip" ]]; then
        echo "❌ VM not running or IP not available" >&2
        return 1
    fi
    
    local guest_os=$(detect_guest_os "$config")
    local ssh_user=$(get_config "$config" '.tart.ssh_user // empty')
    
    if [[ -z "$ssh_user" ]]; then
        case "$guest_os" in
            "macos") ssh_user="admin" ;;
            "linux") ssh_user="ubuntu" ;;
        esac
    fi
    
    # Execute command via SSH
    ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
        "${ssh_user}@${vm_ip}" "$@"
}

# View VM logs
tart_logs() {
    local config="$1"
    local project_dir="$2"
    shift 2
    
    local vm_name=$(get_tart_vm_name "$config")
    
    # Set up custom storage if specified
    setup_tart_storage "$config" >/dev/null 2>&1
    
    # Check if VM exists
    if ! tart list 2>/dev/null | grep -q "^${vm_name}"; then
        echo "❌ VM '$vm_name' does not exist" >&2
        return 1
    fi
    
    # Get VM IP
    local vm_ip=$(tart ip "$vm_name" 2>/dev/null)
    if [[ -z "$vm_ip" ]]; then
        echo "❌ VM not running or IP not available" >&2
        echo "💡 Run 'vm start' first" >&2
        return 1
    fi
    
    local guest_os=$(detect_guest_os "$config")
    local ssh_user=$(get_config "$config" '.tart.ssh_user // empty')
    
    if [[ -z "$ssh_user" ]]; then
        case "$guest_os" in
            "macos") ssh_user="admin" ;;
            "linux") ssh_user="ubuntu" ;;
        esac
    fi
    
    echo "📋 Viewing logs from $vm_name..."
    echo "💡 Press Ctrl+C to stop following logs"
    echo ""
    
    # Based on guest OS, show appropriate logs
    case "$guest_os" in
        "macos")
            # macOS system logs
            ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
                "${ssh_user}@${vm_ip}" "log stream --style compact --info --debug" 2>/dev/null || \
            ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
                "${ssh_user}@${vm_ip}" "tail -f /var/log/system.log" 2>/dev/null || \
            echo "❌ Could not access macOS logs"
            ;;
        "linux")
            # Linux system logs via journalctl or syslog
            ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
                "${ssh_user}@${vm_ip}" "sudo journalctl -f --no-pager" 2>/dev/null || \
            ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
                "${ssh_user}@${vm_ip}" "sudo tail -f /var/log/syslog" 2>/dev/null || \
            echo "❌ Could not access Linux logs"
            ;;
        *)
            echo "❌ Unknown guest OS: $guest_os" >&2
            return 1
            ;;
    esac
}

# Provision VM
tart_provision() {
    local config="$1"
    local project_dir="$2"
    
    echo "📦 Provisioning Tart VM..."
    
    local vm_name=$(get_tart_vm_name "$config")
    local guest_os=$(detect_guest_os "$config")
    
    # Check if VM is running
    local vm_ip=$(tart ip "$vm_name" 2>/dev/null)
    if [[ -z "$vm_ip" ]]; then
        echo "❌ VM must be running to provision" >&2
        echo "💡 Run 'vm start' first" >&2
        return 1
    fi
    
    # Run provisioning based on guest OS
    case "$guest_os" in
        "macos")
            provision_macos_vm "$config" "$vm_name"
            ;;
        "linux")
            provision_linux_vm "$config" "$vm_name"
            ;;
    esac
}

# Provision macOS VM
provision_macos_vm() {
    local config="$1"
    local vm_name="$2"
    
    echo "🍎 Provisioning macOS VM..."
    
    # Create provisioning script
    local provision_script='#!/bin/bash
set -e

echo "📦 Starting macOS provisioning..."

# Install Homebrew if not present
if ! command -v brew >/dev/null 2>&1; then
    echo "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    
    # Add Homebrew to PATH for Apple Silicon
    echo "eval \"\$(/opt/homebrew/bin/brew shellenv)\"" >> ~/.zprofile
    eval "$(/opt/homebrew/bin/brew shellenv)"
fi

# Update Homebrew
echo "Updating Homebrew..."
brew update

# Install basic packages
echo "Installing development tools..."
brew install git htop ripgrep tree wget curl

echo "✅ macOS provisioning complete!"
'
    
    # Execute provisioning script
    if ! tart_exec "$config" "" bash -c "$provision_script"; then
        echo "⚠️  Some provisioning steps may have failed" >&2
    fi
}

# Provision Linux VM
provision_linux_vm() {
    local config="$1"
    local vm_name="$2"
    
    echo "🐧 Provisioning Linux VM..."
    
    # Create provisioning script
    local provision_script='#!/bin/bash
set -e

echo "📦 Starting Linux provisioning..."

# Update package lists
echo "Updating package lists..."
sudo apt-get update

# Install basic packages
echo "Installing development tools..."
sudo apt-get install -y \
    git \
    jq \
    curl \
    wget \
    htop \
    build-essential \
    software-properties-common \
    apt-transport-https \
    ca-certificates \
    gnupg \
    lsb-release

# Install Docker if requested
if [[ "${INSTALL_DOCKER:-false}" == "true" ]]; then
    echo "Installing Docker..."
    curl -fsSL https://get.docker.com | sudo sh
    sudo usermod -aG docker $USER
fi

echo "✅ Linux provisioning complete!"
'
    
    # Check if Docker should be installed
    local install_docker=$(get_config "$config" '.tart.install_docker // false')
    
    # Execute provisioning script
    if ! INSTALL_DOCKER="$install_docker" tart_exec "$config" "" bash -c "$provision_script"; then
        echo "⚠️  Some provisioning steps may have failed" >&2
    fi
}