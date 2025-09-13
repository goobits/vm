#!/bin/bash
# Provider Interface Framework - Unified command routing for Docker and Vagrant
# Purpose: Abstract provider differences and provide consistent command interface
# Version: 1.0.0
# Part of: Docker-Vagrant Parity Enhancement (Phase 1A)

set -e
set -u

# Get shared utilities directory
SHARED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source shared utilities
if [[ -f "$SHARED_DIR/platform-utils.sh" ]]; then
    source "$SHARED_DIR/platform-utils.sh"
fi
if [[ -f "$SHARED_DIR/config-processor.sh" ]]; then
    source "$SHARED_DIR/config-processor.sh"
fi
if [[ -f "$SHARED_DIR/docker-utils.sh" ]]; then
    source "$SHARED_DIR/docker-utils.sh"
fi
if [[ -f "$SHARED_DIR/mount-utils.sh" ]]; then
    source "$SHARED_DIR/mount-utils.sh"
fi
if [[ -f "$SHARED_DIR/security-utils.sh" ]]; then
    source "$SHARED_DIR/security-utils.sh"
fi
if [[ -f "$SHARED_DIR/temporary-file-utils.sh" ]]; then
    source "$SHARED_DIR/temporary-file-utils.sh"
fi

#=============================================================================
# PROVIDER DETECTION AND VALIDATION
#=============================================================================

# Detect the provider from configuration
# Args: config_json
# Returns: provider name (docker|vagrant)
detect_provider() {
    local config="$1"
    get_config_provider "$config"
}

# Validate if a provider is supported
# Args: provider_name
# Returns: 0 if supported, 1 if not
validate_provider() {
    local provider="$1"
    case "$provider" in
        "docker"|"vagrant"|"tart")
            return 0
            ;;
        *)
            echo "❌ Error: Unsupported provider '$provider'. Supported providers: docker, vagrant, tart" >&2
            return 1
            ;;
    esac
}

# Check if provider tools are available on the system
# Args: provider_name
# Returns: 0 if available, 1 if not
is_provider_available() {
    local provider="$1"
    case "$provider" in
        "docker")
            command -v docker >/dev/null 2>&1 && command -v docker-compose >/dev/null 2>&1
            ;;
        "vagrant")
            command -v vagrant >/dev/null 2>&1
            ;;
        "tart")
            # Check for Apple Silicon Mac and Tart installation
            [[ "$(uname -s)" == "Darwin" ]] && \
            [[ "$(uname -m)" == "arm64" ]] && \
            command -v tart >/dev/null 2>&1
            ;;
        *)
            return 1
            ;;
    esac
}

# Get provider capabilities matrix
# Args: provider_name
# Returns: JSON object with capabilities
get_provider_capabilities() {
    local provider="$1"
    case "$provider" in
        "docker")
            echo '{
                "supports_ssh": true,
                "supports_logs": true,
                "supports_exec": true,
                "supports_provision": true,
                "supports_snapshots": false,
                "supports_suspend": false,
                "fast_startup": true,
                "resource_isolation": "container",
                "networking": "bridge"
            }'
            ;;
        "vagrant")
            echo '{
                "supports_ssh": true,
                "supports_logs": true,
                "supports_exec": true,
                "supports_provision": true,
                "supports_snapshots": true,
                "supports_suspend": true,
                "fast_startup": false,
                "resource_isolation": "vm",
                "networking": "nat"
            }'
            ;;
        "tart")
            echo '{
                "supports_ssh": true,
                "supports_logs": true,
                "supports_exec": true,
                "supports_provision": true,
                "supports_snapshots": true,
                "supports_suspend": true,
                "fast_startup": true,
                "resource_isolation": "vm",
                "networking": "nat",
                "guest_os": ["macos", "linux"],
                "host_requirements": "Apple Silicon Mac"
            }'
            ;;
        *)
            echo '{}'
            ;;
    esac
}

#=============================================================================
# COMMAND ROUTING CORE
#=============================================================================

# Main command router - routes commands to appropriate provider
# Args: command, config, project_dir, [additional_args...]
route_command() {
    local command="$1"
    local config="$2"
    local project_dir="$3"
    shift 3

    local provider
    provider=$(detect_provider "$config")

    if ! validate_provider "$provider"; then
        return 1
    fi

    if ! is_provider_available "$provider"; then
        echo "❌ Error: Provider '$provider' is not available on this system" >&2
        echo "💡 Please install the required tools for '$provider' provider" >&2
        return 1
    fi

    case "$provider" in
        "docker")
            docker_command_wrapper "$command" "$config" "$project_dir" "$@"
            ;;
        "vagrant")
            vagrant_command_wrapper "$command" "$config" "$project_dir" "$@"
            ;;
        "tart")
            tart_command_wrapper "$command" "$config" "$project_dir" "$@"
            ;;
        *)
            echo "❌ Error: Unknown provider '$provider'" >&2
            return 1
            ;;
    esac
}

#=============================================================================
# DOCKER PROVIDER WRAPPER
#=============================================================================

# Docker command wrapper with unified error handling
# Args: command, config, project_dir, [additional_args...]
docker_command_wrapper() {
    local command="$1"
    local config="$2"
    local project_dir="$3"
    shift 3

    case "$command" in
        "create"|"up")
            docker_up "$config" "$project_dir" "false" "$@"
            ;;
        "ssh")
            # Calculate relative path (similar to existing vm.sh logic)
            local relative_path="."
            if [[ -n "${CURRENT_DIR:-}" ]] && [[ -n "$project_dir" ]]; then
                relative_path=$(portable_relative_path "$project_dir" "$CURRENT_DIR" 2>/dev/null || echo ".")
            fi
            docker_ssh "$config" "$project_dir" "$relative_path" "$@"
            ;;
        "start"|"resume")
            docker_start "$config" "$project_dir" "$@"
            ;;
        "stop"|"halt")
            docker_halt "$config" "$project_dir" "$@"
            ;;
        "restart"|"reload")
            docker_reload "$config" "$project_dir" "$@"
            ;;
        "destroy")
            docker_destroy "$config" "$project_dir" "$@"
            ;;
        "status")
            docker_status "$config" "$project_dir" "$@"
            ;;
        "provision")
            docker_provision "$config" "$project_dir" "$@"
            ;;
        "logs")
            docker_logs "$config" "$project_dir" "$@"
            ;;
        "exec")
            docker_exec "$config" "$project_dir" "$@"
            ;;
        "kill")
            docker_kill "$config" "$project_dir" "$@"
            ;;
        *)
            echo "❌ Error: Unknown command '$command' for Docker provider" >&2
            return 1
            ;;
    esac
}

#=============================================================================
# VAGRANT PROVIDER WRAPPER
#=============================================================================

# Vagrant command wrapper with unified error handling
# Args: command, config, project_dir, [additional_args...]
vagrant_command_wrapper() {
    local command="$1"
    local config="$2"
    local project_dir="$3"
    shift 3

    # Set up Vagrant environment
    local vagrant_dir="$SCRIPT_DIR/../providers/vagrant"
    export VAGRANT_CWD="$vagrant_dir"
    export VM_PROJECT_DIR="$project_dir"

    # Set VM_CONFIG if we have a full config path
    if [[ -n "${FULL_CONFIG_PATH:-}" ]]; then
        export VM_CONFIG="$FULL_CONFIG_PATH"
    fi

    case "$command" in
        "create"|"up")
            vagrant_create "$config" "$project_dir" "$@"
            ;;
        "ssh")
            vagrant_ssh "$config" "$project_dir" "$@"
            ;;
        "start"|"resume")
            vagrant start resume "$@" || vagrant up "$@"
            ;;
        "stop"|"halt")
            vagrant halt "$@"
            ;;
        "restart"|"reload")
            vagrant_restart "$config" "$project_dir" "$@"
            ;;
        "destroy")
            vagrant destroy -f "$@"
            ;;
        "status")
            vagrant status "$@"
            ;;
        "provision")
            vagrant provision "$@"
            ;;
        "logs")
            vagrant_logs "$config" "$project_dir" "$@"
            ;;
        "exec")
            vagrant_exec "$config" "$project_dir" "$@"
            ;;
        *)
            # Pass through to vagrant command for unknown commands
            vagrant "$command" "$@"
            ;;
    esac
}

#=============================================================================
# VAGRANT HELPER FUNCTIONS
#=============================================================================

# Vagrant create command with confirmation logic
vagrant_create() {
    local config="$1"
    local project_dir="$2"
    shift 2

    # Check if VM already exists and confirm before recreating
    if vagrant status default 2>/dev/null | grep -q "running\|poweroff\|saved"; then
        echo "⚠️  Vagrant VM already exists."
        echo -n "Are you sure you want to recreate it? This will destroy the existing VM and all its data. (y/N): "
        read -r response
        case "$response" in
            [yY]|[yY][eE][sS])
                echo "🗑️  Destroying existing VM first..."
                vagrant destroy -f
                ;;
            *)
                echo "❌ VM creation cancelled."
                return 1
                ;;
        esac
    fi

    # Start VM
    vagrant up "$@"

    echo "💡 Use 'vm ssh' to connect to the VM"
}

# Vagrant SSH with relative path support
vagrant_ssh() {
    local config="$1"
    local project_dir="$2"
    shift 2

    # Calculate relative path (similar to existing vm.sh logic)
    local relative_path="."
    if [[ -n "${CURRENT_DIR:-}" ]]; then
        if [[ "${CUSTOM_CONFIG:-}" = "__SCAN__" ]]; then
            # In scan mode, figure out where we are relative to the found config
            local config_dir
            config_dir=$(echo "$config" | jq -r '.__config_dir // empty' 2>/dev/null)
            if [[ -n "$config_dir" ]] && [[ "$config_dir" != "$CURRENT_DIR" ]]; then
                relative_path=$(portable_relative_path "$config_dir" "$CURRENT_DIR" 2>/dev/null || echo ".")
            fi
        else
            # Normal mode: relative path from project dir to current dir
            relative_path=$(portable_relative_path "$project_dir" "$CURRENT_DIR" 2>/dev/null || echo ".")
        fi
    fi

    # Get workspace path from config
    local workspace_path
    workspace_path=$(echo "$config" | jq -r '.project.workspace_path // "/workspace"' 2>/dev/null)

    if [[ "$relative_path" != "." ]]; then
        local target_dir="${workspace_path}/${relative_path}"
        vagrant ssh -c "cd $(printf '%q' \"$target_dir\") && exec /bin/zsh"
    else
        vagrant ssh "$@"
    fi
}

# Vagrant restart implementation
vagrant_restart() {
    local config="$1"
    local project_dir="$2"
    shift 2

    vagrant halt "$@"
    vagrant resume "$@" || vagrant up "$@"
}

# Vagrant logs implementation
vagrant_logs() {
    local config="$1"
    local project_dir="$2"
    shift 2

    echo "Showing service logs - Press Ctrl+C to stop..."
    vagrant ssh -c "sudo journalctl -u postgresql -u redis-server -u mongod -f"
}

# Vagrant exec implementation
vagrant_exec() {
    local config="$1"
    local project_dir="$2"
    shift 2

    # Escape all arguments individually for safe passing to vagrant ssh -c
    local escaped_command=""
    for arg in "$@"; do
        escaped_command="$escaped_command $(printf '%q' "$arg")"
    done
    vagrant ssh -c "$escaped_command"
}

#=============================================================================
# TART PROVIDER WRAPPER
#=============================================================================

# Tart command wrapper for Apple Silicon Macs
# Args: command, config, project_dir, [additional_args...]
tart_command_wrapper() {
    local command="$1"
    local config="$2"
    local project_dir="$3"
    shift 3
    
    # Source Tart provider implementation
    local tart_provider_script="$SCRIPT_DIR/providers/tart/tart-provider.sh"
    if [[ ! -f "$tart_provider_script" ]]; then
        echo "❌ Error: Tart provider implementation not found" >&2
        echo "💡 Expected location: $tart_provider_script" >&2
        return 1
    fi
    
    source "$tart_provider_script"
    
    # Execute the command via Tart provider
    tart_command_wrapper_impl "$command" "$config" "$project_dir" "$@"
}

#=============================================================================
# UNIFIED PROVIDER INTERFACE
#=============================================================================

# Unified VM create command
# Args: config, project_dir, [additional_args...]
vm_create() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "create" "$config" "$project_dir" "$@"
}

# Unified VM SSH command
# Args: config, project_dir, [additional_args...]
vm_ssh() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "ssh" "$config" "$project_dir" "$@"
}

# Unified VM start command
# Args: config, project_dir, [additional_args...]
vm_start() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "start" "$config" "$project_dir" "$@"
}

# Unified VM halt command
# Args: config, project_dir, [additional_args...]
vm_halt() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "halt" "$config" "$project_dir" "$@"
}

# Unified VM destroy command
# Args: config, project_dir, [additional_args...]
vm_destroy() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "destroy" "$config" "$project_dir" "$@"
}

# Unified VM status command
# Args: config, project_dir, [additional_args...]
vm_status() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "status" "$config" "$project_dir" "$@"
}

# Unified VM restart command
# Args: config, project_dir, [additional_args...]
vm_restart() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "restart" "$config" "$project_dir" "$@"
}

# Unified VM provision command
# Args: config, project_dir, [additional_args...]
vm_provision() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "provision" "$config" "$project_dir" "$@"
}

# Unified VM logs command
# Args: config, project_dir, [additional_args...]
vm_logs() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "logs" "$config" "$project_dir" "$@"
}

# Unified VM exec command
# Args: config, project_dir, [additional_args...]
vm_exec() {
    local config="$1"
    local project_dir="$2"
    shift 2

    route_command "exec" "$config" "$project_dir" "$@"
}

#=============================================================================
# PROVIDER INFORMATION UTILITIES
#=============================================================================

# Show provider information and capabilities
# Args: config
show_provider_info() {
    local config="$1"
    local provider
    provider=$(detect_provider "$config")

    echo "🔧 Provider: $provider"

    if is_provider_available "$provider"; then
        echo "✅ Provider tools are available"
        local capabilities
        capabilities=$(get_provider_capabilities "$provider")
        echo "📋 Capabilities:"
        echo "$capabilities" | jq -r 'to_entries[] | "  \(.key): \(.value)"' 2>/dev/null || echo "  (capabilities info unavailable)"
    else
        echo "❌ Provider tools are not available"
    fi
}

# Provider health check
# Args: provider_name
provider_health_check() {
    local provider="$1"

    echo "🔍 Checking $provider provider health..."

    case "$provider" in
        "docker")
            if command -v docker >/dev/null 2>&1; then
                echo "✅ Docker command available"
                if docker info >/dev/null 2>&1; then
                    echo "✅ Docker daemon is running"
                else
                    echo "❌ Docker daemon is not running"
                    return 1
                fi
            else
                echo "❌ Docker command not found"
                return 1
            fi

            if command -v docker-compose >/dev/null 2>&1; then
                echo "✅ Docker Compose available"
            else
                echo "❌ Docker Compose not found"
                return 1
            fi
            ;;
        "vagrant")
            if command -v vagrant >/dev/null 2>&1; then
                echo "✅ Vagrant command available"
                vagrant version 2>/dev/null || echo "⚠️  Could not get Vagrant version"
            else
                echo "❌ Vagrant command not found"
                return 1
            fi
            ;;
        *)
            echo "❌ Unknown provider: $provider"
            return 1
            ;;
    esac

    echo "✅ Provider $provider health check completed"
}

#=============================================================================
# ERROR HANDLING UTILITIES
#=============================================================================

# Standardized error message format
provider_error() {
    local provider="$1"
    local command="$2"
    local message="$3"
    local exit_code="${4:-1}"

    echo "❌ Error: $provider provider failed to execute '$command'" >&2
    if [[ -n "$message" ]]; then
        echo "💡 $message" >&2
    fi
    return "$exit_code"
}

# Check if required functions exist (for debugging)
check_provider_functions() {
    local provider="$1"
    local missing_functions=()

    case "$provider" in
        "docker")
            local required_functions=(
                "docker_up" "docker_ssh" "docker_start" "docker_halt"
                "docker_destroy" "docker_status" "docker_reload"
                "docker_provision" "docker_logs" "docker_exec" "docker_kill"
            )
            ;;
        "vagrant")
            # Vagrant uses direct vagrant commands, no custom functions required
            return 0
            ;;
        *)
            echo "❌ Unknown provider: $provider" >&2
            return 1
            ;;
    esac

    for func in "${required_functions[@]}"; do
        if ! declare -f "$func" >/dev/null 2>&1; then
            missing_functions+=("$func")
        fi
    done

    if [[ ${#missing_functions[@]} -gt 0 ]]; then
        echo "❌ Missing required functions for $provider provider:" >&2
        printf '  %s\n' "${missing_functions[@]}" >&2
        return 1
    fi

    echo "✅ All required functions available for $provider provider"
    return 0
}