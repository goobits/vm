#!/bin/bash
# OS-based Configuration Processor
# Intelligently configures VMs based on desired OS and respects user's vm settings

set -e

# Source utilities
OS_CONFIG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$OS_CONFIG_DIR/platform-utils.sh"

# Initialize Rust binary paths (these are bundled with the project)
VM_CONFIG="$OS_CONFIG_DIR/../rust/target/release/vm-config"

# Detect required OS from config or project
detect_required_os() {
    local config="$1"
    local project_dir="${2:-$(pwd)}"
    
    # Check if explicitly specified in config
    local os=""
    if [[ -n "$config" ]]; then
        local temp_config=$(mktemp)
        echo "$config" > "$temp_config"
        os=$("$OS_CONFIG_DIR/../rust/vm-config/target/release/vm-config" query "$temp_config" "os" --raw 2>/dev/null || echo "")
        rm -f "$temp_config"
    fi
    
    if [[ -n "$os" ]] && [[ "$os" != "auto" ]]; then
        echo "$os"
        return
    fi
    
    # Auto-detect from project files
    if [[ -d "$project_dir" ]]; then
        # iOS/macOS development
        if find "$project_dir" -maxdepth 2 -name "*.xcodeproj" -o -name "*.xcworkspace" - 2>/dev/null || echo '' | grep -q .; then
            echo "macos"
            return
        fi
        
        # Swift Package
        if [[ -f "$project_dir/Package.swift" ]]; then
            echo "macos"
            return
        fi
        
        # Has specific Linux preference
        if [[ -f "$project_dir/Dockerfile" ]]; then
            # Check what the Dockerfile uses
            local base_image=$(grep "^FROM" "$project_dir/Dockerfile" - 2>/dev/null || echo '' | head -1 || true)
            case "$base_image" in
                *alpine*) echo "alpine" ;;
                *debian*) echo "debian" ;;
                *ubuntu*) echo "ubuntu" ;;
                *) echo "ubuntu" ;;
            esac
            return
        fi
    fi
    
    # Default to Ubuntu for general development
    echo "ubuntu"
}

# Get OS-specific defaults that can be overridden by vm options
get_os_defaults() {
    local os="$1"
    local user_config="$2"
    
    # Extract user's vm settings if provided
    local user_memory="" user_cpus="" user_disk="" user_username=""
    if [[ -n "$user_config" ]]; then
        local temp_config=$(mktemp)
        echo "$user_config" > "$temp_config"
        user_memory=$("$OS_CONFIG_DIR/../rust/vm-config/target/release/vm-config" query "$temp_config" "vm.memory" --raw 2>/dev/null || echo "")
        user_cpus=$("$OS_CONFIG_DIR/../rust/vm-config/target/release/vm-config" query "$temp_config" "vm.cpus" --raw 2>/dev/null || echo "")
        user_disk=$("$OS_CONFIG_DIR/../rust/vm-config/target/release/vm-config" query "$temp_config" "vm.disk_size" --raw 2>/dev/null || echo "")
        user_username=$("$OS_CONFIG_DIR/../rust/vm-config/target/release/vm-config" query "$temp_config" "vm.user" --raw 2>/dev/null || echo "")
        rm -f "$temp_config"
    fi
    
    case "$os" in
        macos)
            # macOS needs more resources, but respect user overrides
            local memory="${user_memory:-8192}"
            local cpus="${user_cpus:-4}"
            local disk="${user_disk:-60}"
            local username="${user_username:-admin}"
            
            # Check if user specified a storage path
            local user_storage_path=""
            if [[ -n "$user_config" ]]; then
                local temp_config=$(mktemp)
                echo "$user_config" > "$temp_config"
                user_storage_path=$("$OS_CONFIG_DIR/../rust/vm-config/target/release/vm-config" query "$temp_config" "tart.storage_path" --raw 2>/dev/null || echo "")
                rm -f "$temp_config"
            fi
            local storage_json=""
            if [[ -n "$user_storage_path" ]] && [[ "$user_storage_path" != "null" ]]; then
                storage_json=",\"storage_path\": \"$user_storage_path\""
            fi
            
            echo "{
                \"provider\": \"tart\",
                \"vm\": {
                    \"memory\": $memory,
                    \"cpus\": $cpus,
                    \"user\": \"$username\"
                },
                \"tart\": {
                    \"guest_os\": \"macos\",
                    \"image\": \"ghcr.io/cirruslabs/macos-sonoma-base:latest\",
                    \"disk_size\": $disk,
                    \"ssh_user\": \"admin\"$storage_json
                },
                \"project\": {
                    \"workspace_path\": \"/Users/admin/workspace\"
                }
            }"
            ;;
            
        ubuntu)
            # Ubuntu with reasonable defaults
            local memory="${user_memory:-4096}"
            local cpus="${user_cpus:-2}"
            local username="${user_username:-developer}"
            
            echo "{
                \"provider\": \"docker\",
                \"vm\": {
                    \"memory\": $memory,
                    \"cpus\": $cpus,
                    \"user\": \"$username\"
                }
            }"
            ;;
            
        debian)
            # Debian - similar to Ubuntu but might use different packages
            local memory="${user_memory:-2048}"
            local cpus="${user_cpus:-2}"
            local username="${user_username:-developer}"
            
            echo "{
                \"provider\": \"docker\",
                \"vm\": {
                    \"memory\": $memory,
                    \"cpus\": $cpus,
                    \"user\": \"$username\"
                }
            }"
            ;;
            
        alpine)
            # Alpine - minimal resources
            local memory="${user_memory:-1024}"
            local cpus="${user_cpus:-1}"
            local username="${user_username:-developer}"
            
            echo "{
                \"provider\": \"docker\",
                \"vm\": {
                    \"memory\": $memory,
                    \"cpus\": $cpus,
                    \"user\": \"$username\"
                }
            }"
            ;;
            
        linux)
            # Generic Linux - use Ubuntu as base
            local memory="${user_memory:-4096}"
            local cpus="${user_cpus:-2}"
            local username="${user_username:-developer}"
            
            echo "{
                \"provider\": \"docker\",
                \"vm\": {
                    \"memory\": $memory,
                    \"cpus\": $cpus,
                    \"user\": \"$username\"
                }
            }"
            ;;
            
        *)
            # Unknown OS - safe defaults
            echo "{
                \"provider\": \"docker\",
                \"vm\": {
                    \"memory\": 2048,
                    \"cpus\": 2,
                    \"user\": \"developer\"
                }
            }"
            ;;
    esac
}

# Select best provider for the OS and host combination
select_provider_for_os() {
    local os="$1"
    local host_os="$(uname -s)"
    local host_arch="$(uname -m)"
    
    case "$os" in
        macos)
            # macOS can only run on Apple Silicon with Tart
            if [[ "$host_os" == "Darwin" ]] && [[ "$host_arch" == "arm64" ]]; then
                if command -v tart >/dev/null 2>&1; then
                    echo "tart"
                else
                    echo "error:Tart not installed. Run: brew install cirruslabs/cli/tart"
                fi
            else
                echo "error:macOS VMs require Apple Silicon Mac"
            fi
            ;;
            
        ubuntu|debian|alpine|linux)
            # Linux can run on multiple providers, pick the best
            if command -v docker >/dev/null 2>&1; then
                # Docker is fastest for Linux containers
                echo "docker"
            elif [[ "$host_os" == "Darwin" ]] && [[ "$host_arch" == "arm64" ]] && command -v tart >/dev/null 2>&1; then
                # Can use Tart for Linux on Apple Silicon
                echo "tart"
            elif command -v vagrant >/dev/null 2>&1; then
                # Fallback to Vagrant
                echo "vagrant"
            else
                echo "error:No suitable virtualization provider found"
            fi
            ;;
            
        *)
            echo "error:Unknown OS: $os"
            ;;
    esac
}

# Apply OS-based configuration to existing config
apply_os_config() {
    local config="$1"
    local os="$2"
    
    # Get OS defaults
    local os_defaults=$(get_os_defaults "$os" "$config")
    
    # Select provider if not explicitly set
    local provider=""
    if [[ -n "$config" ]]; then
        local temp_config=$(mktemp)
        echo "$config" > "$temp_config"
        provider=$("$OS_CONFIG_DIR/../rust/vm-config/target/release/vm-config" query "$temp_config" "provider" --raw 2>/dev/null || echo "")
        rm -f "$temp_config"
    fi
    if [[ -z "$provider" ]] || [[ "$provider" == "auto" ]]; then
        provider=$(select_provider_for_os "$os")
        
        # Check for errors
        if [[ "$provider" == error:* ]]; then
            echo "❌ ${provider#error:}" >&2
            return 1
        fi
    fi
    
    # Merge configurations: user config overrides OS defaults
    # This ensures user's vm settings are respected
    local temp_defaults=$(mktemp)
    local temp_user=$(mktemp)
    local temp_merged=$(mktemp)
    echo "$os_defaults" > "$temp_defaults"
    echo "$config" > "$temp_user"

    # Use vm-config to merge configurations
    if "$VM_CONFIG" merge --base "$temp_defaults" --overlay "$temp_user" -f yaml > "$temp_merged"; then
        # Set the detected/selected provider in the merged config
        echo "provider: $provider" >> "$temp_merged"
        cat "$temp_merged"
    else
        # Fallback: use OS defaults with provider
        echo "$os_defaults" | sed "s/provider:.*/provider: $provider/"
    fi

    rm -f "$temp_defaults" "$temp_user" "$temp_merged"
}

# Main function to process OS-based configuration
process_os_config() {
    local config_file="${1:-vm.yaml}"
    local project_dir="${2:-$(pwd)}"
    
    # Read config
    local config
    if [[ -f "$config_file" ]]; then
        # Read YAML file directly - no conversion needed
        config=$(cat "$config_file")
    else
        config=''
    fi
    
    # Detect OS
    local os=$(detect_required_os "$config" "$project_dir")
    
    # Apply OS configuration
    local final_config=$(apply_os_config "$config" "$os")
    
    echo "$final_config"
}

# Check if OS is compatible with current system
check_os_compatibility() {
    local os="$1"
    local provider=$(select_provider_for_os "$os")
    
    if [[ "$provider" == error:* ]]; then
        return 1
    fi
    
    return 0
}

# Get recommended resources for OS
get_os_recommended_resources() {
    local os="$1"
    
    case "$os" in
        macos)
            echo "Memory: 8GB, CPUs: 4, Disk: 60GB"
            ;;
        ubuntu|debian)
            echo "Memory: 4GB, CPUs: 2, Disk: 30GB"
            ;;
        alpine)
            echo "Memory: 1GB, CPUs: 1, Disk: 10GB"
            ;;
        *)
            echo "Memory: 2GB, CPUs: 2, Disk: 20GB"
            ;;
    esac
}