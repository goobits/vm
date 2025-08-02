#!/bin/bash
# VM wrapper script for Goobits - supports both Vagrant and Docker
# Usage: ./packages/vm/vm.sh [command] [args...]

# Check if being sourced for functions only
if [[ "$1" == "--source-only" ]]; then
    # Don't execute main logic, just define functions
    # Skip to the end after function definitions
    SKIP_MAIN_EXECUTION=true
fi

set -e

# Enable debug mode if VM_DEBUG is set
if [[ "${VM_DEBUG:-}" = "true" ]]; then
    set -x
fi

# Check for required tools
if ! command -v yq &> /dev/null; then
    echo "❌ Error: yq is not installed. This tool is required for YAML processing."
    echo ""
    echo "📦 To install yq on Ubuntu/Debian:"
    echo "   sudo apt-get update"
    echo "   sudo apt-get install yq"
    echo ""
    echo "📦 To install yq on macOS:"
    echo "   brew install yq"
    echo ""
    echo "📦 To install yq on other systems:"
    echo "   Visit: https://github.com/kislyuk/yq"
    echo ""
    exit 1
fi

# Default port configuration (removed unused variables)

# Get the directory where this script is located (packages/vm)
# Handle both direct execution and npm link scenarios
if [[ -L "$0" ]]; then
    # If this is a symlink (npm link), resolve the real path
    REAL_SCRIPT="$(readlink -f "$0")"
    SCRIPT_DIR="$(cd "$(dirname "$REAL_SCRIPT")" && pwd)"
else
    # Direct execution
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Get the current working directory (where user ran the command)
CURRENT_DIR="$(pwd)"

# Source shared utilities
source "$SCRIPT_DIR/shared/npm-utils.sh"
source "$SCRIPT_DIR/shared/docker-utils.sh"

# Validate mount directory security (dangerous characters and path traversal)
validate_mount_security() {
    local dir_path="$1"

    # Check for dangerous characters
    if [[ "$dir_path" =~ [\;\`\$\"] ]]; then
        echo "❌ Error: Directory name contains potentially dangerous characters" >&2
        echo "💡 Directory names cannot contain: ; \` $ \"" >&2
        return 1
    fi

    # Check for path traversal attempts
    if [[ "$dir_path" =~ \.\./ ]] || [[ "$dir_path" =~ /\.\. ]]; then
        echo "❌ Error: Directory path traversal not allowed" >&2
        return 1
    fi

    return 0
}

# Detect potential comma-in-directory-name issues by analyzing parsed fragments
detect_comma_in_paths() {
    local -n mounts_array="$1"
    local suspicious_count=0
    local total_count=${#mounts_array[@]}

    # Check if any parsed fragment looks suspicious (very short, no path separators)
    for test_mount in "${mounts_array[@]}"; do
        test_mount=$(echo "$test_mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        # Remove permission suffix for testing
        if [[ "$test_mount" == *:* ]]; then
            test_mount="${test_mount%:*}"
        fi
        # Very short fragments (1-2 chars) without slashes are suspicious
        if [[ -n "$test_mount" ]] && [[ ${#test_mount} -le 2 ]] && [[ "$test_mount" != *"/"* ]] && [[ "$test_mount" != "."* ]]; then
            ((suspicious_count++))
        fi
    done

    # If more than half the fragments are suspicious short names, likely comma issue
    if [[ $total_count -gt 2 ]] && [[ $suspicious_count -gt $((total_count / 2)) ]]; then
        echo "❌ Error: Possible comma-containing directory names detected" >&2
        echo "   Parsed fragments: ${mounts_array[*]}" >&2
        echo "   Directory names containing commas are not supported" >&2
        echo "   Tip: Use symlinks like: ln -s 'dir,with,commas' dir-without-commas" >&2
        return 1
    fi

    return 0
}

# Parse mount permissions and return appropriate Docker mount flags
parse_mount_permissions() {
    local perm="$1"
    local mount_flags=""

    case "$perm" in
        "ro"|"readonly")
            mount_flags=":ro"
            ;;
        "rw"|"readwrite"|*)
            # Default to read-write (no additional flags)
            mount_flags=""
            ;;
    esac

    echo "$mount_flags"
}

# Construct Docker mount argument for a validated directory and permissions
construct_mount_argument() {
    local source_dir="$1"
    local permission_flags="$2"

    # Validate and quote the realpath to prevent command injection
    local real_source
    if ! real_source=$(realpath "$source_dir" 2>/dev/null); then
        echo "❌ Error: Cannot resolve path '$source_dir'" >&2
        return 1
    fi

    # Build the mount argument
    echo "-v $(printf '%q' "$real_source"):/workspace/$(basename "$source_dir")${permission_flags}"
}

# Process a single mount specification (with or without permissions)
process_single_mount() {
    local mount="$1"
    local source=""
    local perm=""

    # Trim whitespace
    mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

    # Handle mount:permission format (e.g., ./src:rw, ./config:ro)
    if [[ "$mount" == *:* ]]; then
        source="${mount%:*}"
        perm="${mount##*:}"
    else
        source="$mount"
        perm="rw"  # Default to read-write
    fi

    # Check if source exists and is a directory
    if [[ ! -d "$source" ]]; then
        echo "❌ Error: Directory '$source' does not exist or is not a directory" >&2
        return 1
    fi

    # Validate directory security
    if ! validate_mount_security "$source"; then
        return 1
    fi

    # Parse permissions and construct mount argument
    local permission_flags
    permission_flags=$(parse_mount_permissions "$perm")

    # Construct and return the mount argument
    construct_mount_argument "$source" "$permission_flags"
}

# Parse comma-separated mount string into mount arguments
# Note: Directory names containing commas are not supported due to parsing complexity
parse_mount_string() {
    local mount_str="$1"
    local mount_args=""

    if [[ -n "$mount_str" ]]; then
        # Split by comma and process each mount (save original IFS)
        local old_ifs="$IFS"
        IFS=','
        local MOUNTS
        IFS=',' read -r -a MOUNTS <<< "$mount_str"  # Proper array assignment
        IFS="$old_ifs"

        # Pre-validate: Detect comma-in-directory-name issues
        if ! detect_comma_in_paths MOUNTS; then
            return 1
        fi

        # Process each mount using dedicated sub-function
        for mount in "${MOUNTS[@]}"; do
            local mount_arg
            if mount_arg=$(process_single_mount "$mount"); then
                mount_args="$mount_args $mount_arg"
            else
                return 1
            fi
        done
    fi

    echo "$mount_args"
}

# Docker utility functions moved to shared/docker-utils.sh


# Show usage information
show_usage() {
    # Try to get version from package.json
    local version
    version=""
    if [[ -f "$SCRIPT_DIR/package.json" ]]; then
        version=$(grep '"version"' "$SCRIPT_DIR/package.json" | head -1 | cut -d'"' -f4)
    fi

    if [[ -n "$version" ]]; then
        echo "VM Tool v$version"
        echo ""
    fi

    echo "Usage: $0 [--config [PATH]] [--debug] [--dry-run] [--auto-login [true|false]] [command] [args...]"
    echo ""
    echo "Options:"
    echo "  --config [PATH]      Use specific vm.yaml file, or scan up directory tree if no path given"
    echo "  --debug              Enable debug output"
    echo "  --dry-run            Show what would be executed without actually running it"
    echo "  --auto-login [BOOL]  Automatically SSH into VM after create/start (default: true)"
    echo ""
    echo "Commands:"
    echo "  init                  Initialize a new vm.yaml configuration file"
    echo "  generate              Generate vm.yaml by composing services"
    echo "  validate              Validate VM configuration"
    echo "  migrate [options]     Migrate a legacy vm.json to the new vm.yaml format"
    echo "  list                  List all VM instances"
    echo "  temp/tmp <command>    Manage temporary VMs:"
    echo "    <mounts> [--auto-destroy]  Create/connect temp VM with directory mounts"
    echo "    ssh                        SSH into the active temp VM"
    echo "    status                     Show status of the active temp VM"
    echo "    destroy                    Destroy the active temp VM"
    echo "  create [args]         Create new VM with full provisioning"
    echo "  start [args]          Start existing VM without provisioning"
    echo "  stop [args]           Stop VM but keep data"
    echo "  restart [args]        Restart VM without reprovisioning"
    echo "  ssh [args]            SSH into VM"
    echo "  destroy [args]        Destroy VM completely"
    echo "  status [args]         Check VM status"
    echo "  provision [args]      Re-run full provisioning on existing VM"
    echo "  logs [args]           View VM logs (Docker only)"
    echo "  exec [args]           Execute command in VM (Docker only)"
    echo "  test [args]           Run VM test suite"
    echo "  get-sync-directory    Get current VM directory for shell integration"
    echo "  kill                  Force kill VM processes"
    echo ""
    echo "Examples:"
    echo "  vm generate --services postgresql,redis  # Generate config with services"
    echo "  vm generate --ports 3020 --name my-app   # Generate with custom ports/name"
    echo "  vm validate                              # Check configuration"
    echo "  vm list                                  # List all VM instances"
    echo "  vm temp ./client,./server,./shared       # Create temp VM with specific folders"
    echo "  vm temp ./src:rw,./config:ro             # Temp VM with mount permissions"
    echo "  vm temp ./src --auto-destroy             # Temp VM that destroys on exit"
    echo "  vm temp ssh                              # SSH into active temp VM"
    echo "  vm temp status                           # Check temp VM status"
    echo "  vm temp destroy                          # Destroy temp VM"
    echo "  vm tmp ./src                             # 'tmp' is an alias for 'temp'"
    echo "  vm --config ./prod.yaml create           # Create VM with specific config"
    echo "  vm --config create                       # Create VM scanning up for vm.yaml"
    echo "  vm create                                # Create new VM (auto-find vm.yaml)"
    echo "  vm create --auto-login=false             # Create VM without auto SSH"
    echo "  vm start                                 # Start existing VM (fast)"
    echo "  vm start --auto-login=false              # Start VM without auto SSH"
    echo "  vm ssh                                   # Connect to VM"
    echo "  vm stop                                  # Stop the VM"
    echo "  vm get-sync-directory                    # Get VM's current directory for cd"
    echo ""
    echo "The provider (Vagrant or Docker) is determined by the 'provider' field in vm.yaml"
}

# Function to kill VirtualBox processes
kill_virtualbox() {
    echo "🔄 Terminating all VirtualBox processes..."

    # Force kill VirtualBox and ALL related processes
    echo "🔪 Force killing ALL VirtualBox processes..."
    pkill -9 -f "VBoxHeadless" || true
    pkill -9 -f "VBoxSVC" || true
    pkill -9 -f "VBoxXPCOMIPCD" || true
    pkill -9 -f "VirtualBox" || true

    echo "⏳ Waiting for VirtualBox services to terminate..."
    sleep 3

    echo "✅ All VirtualBox processes terminated!"
    echo ""
    echo "ℹ️ You may now need to manually clean up in the VirtualBox application"
    echo "ℹ️ or run 'vagrant up' to start your VM again."
}

# Function to load and validate config (delegated to validate-config.sh)
load_config() {
    local config_path="$1"
    local original_dir="$2"

    # Debug output if --debug flag is set
    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "DEBUG load_config: config_path='$config_path', original_dir='$original_dir'" >&2
        echo "DEBUG load_config: SCRIPT_DIR='$SCRIPT_DIR'" >&2
    fi

    if [[ -n "$config_path" ]]; then
        # Use custom config path
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG load_config: Running: cd '$original_dir' && '$SCRIPT_DIR/validate-config.sh' --get-config '$config_path'" >&2
        fi
        (cd "$original_dir" && "$SCRIPT_DIR/validate-config.sh" --get-config "$config_path")
    else
        # Use default discovery logic - run from the original directory
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG load_config: Running: cd '$original_dir' && '$SCRIPT_DIR/validate-config.sh' --get-config" >&2
        fi
        (cd "$original_dir" && "$SCRIPT_DIR/validate-config.sh" --get-config)
    fi
}


# Get provider from config
get_provider() {
    local config="$1"
    echo "$config" | yq -r '.provider // "docker"'
}

# Extract project name from config
# This centralizes the project name extraction logic to reduce duplication
get_project_name() {
    local config="$1"
    echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]'
}

# Extract project name from config and generate container name
# This centralizes the logic for creating container names to reduce duplication
get_project_container_name() {
    local config="$1"
    local project_name
    project_name=$(get_project_name "$config")
    echo "${project_name}-dev"
}

# Docker helper function to reduce duplication
docker_run() {
    local action="$1"
    local config="$2"
    local project_dir="$3"
    shift 3

    # Get container name using shared function
    local container_name
    container_name=$(get_project_container_name "$config")

    case "$action" in
        "compose")
            cd "$project_dir"
            docker_compose "$@"
            ;;
        "exec")
            docker_cmd exec "${container_name}" "$@"
            ;;
        "exec-it")
            docker_cmd exec -it "${container_name}" "$@"
            ;;
        *)
            cd "$project_dir"
            docker_compose "$action" "$@"
            ;;
    esac
}

# Docker functions
docker_up() {
    local config="$1"
    local project_dir="$2"
    local auto_login="$3"
    shift 3

    echo "🚀 Starting development environment..."

    # Create a secure temporary file for the config
    TEMP_CONFIG_FILE=$(mktemp /tmp/vm-config.XXXXXX)
    # Ensure the temp file is removed when the script exits
    trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT

    # Generate docker-compose.yml
    echo "$config" > "$TEMP_CONFIG_FILE"
    "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"

    # Build and start containers
    docker_run "compose" "$config" "$project_dir" build
    docker_run "compose" "$config" "$project_dir" up -d "$@"

    # Get container name using shared function
    local container_name
    container_name=$(get_project_container_name "$config")

    # Wait for container to be ready before proceeding
    echo "⏳ Initializing container..."
    local max_attempts=30
    local attempt=1
    while [[ $attempt -le $max_attempts ]]; do
        # Use docker_cmd to handle sudo if needed, and check container is running
        if docker_cmd inspect "${container_name}" --format='{{.State.Status}}' 2>/dev/null | grep -q "running"; then
            # Also verify we can exec into it
            if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
                echo "✅ Container is ready"
                break
            fi
        fi
        if [[ $attempt -eq $max_attempts ]]; then
            echo "❌ Environment initialization failed"
            return 1
        fi
        echo "⏳ Starting up... ($attempt/$max_attempts)"
        sleep 2
        ((attempt++))
    done

    # Copy config file to container
    echo "📋 Loading project configuration..."
    if docker_cmd cp "$TEMP_CONFIG_FILE" "${container_name}:/tmp/vm-config.json"; then
        echo "✅ Configuration loaded"
    else
        echo "❌ Configuration loading failed"
        return 1
    fi

    # Fix volume permissions before Ansible
    echo "🔑 Setting up permissions..."
    local project_user
    project_user=$(echo "$config" | yq -r '.vm.user // "developer"')
    if docker_run "exec" "$config" "$project_dir" chown -R "$project_user:$project_user" "/home/$project_user/.nvm" "/home/$project_user/.cache"; then
        echo "✅ Permissions configured"
    else
        echo "⚠️ Permission setup skipped (non-critical)"
    fi

    # VM tool directory is already mounted read-only via docker-compose

    # Run Ansible playbook inside the container
    echo "🔧 Provisioning development environment..."

    # Check if debug mode is enabled
    ANSIBLE_VERBOSITY=""
    ANSIBLE_DIFF=""
    if [[ "${VM_DEBUG:-}" = "true" ]] || [[ "${DEBUG:-}" = "true" ]]; then
        echo "🐛 Debug mode enabled - showing detailed Ansible output"
        ANSIBLE_VERBOSITY="-vvv"
        ANSIBLE_DIFF="--diff"
    fi

    # Create log file path
    ANSIBLE_LOG="/tmp/ansible-provision-$(date +%Y%m%d-%H%M%S).log"

    if docker_run "exec" "$config" "$project_dir" bash -c "ansible-playbook \
        -i localhost, \
        -c local \
        $ANSIBLE_VERBOSITY \
        $ANSIBLE_DIFF \
        /vm-tool/shared/ansible/playbook.yml 2>&1 | tee $ANSIBLE_LOG"; then
        echo "🎉 Development environment ready!"
    else
        ANSIBLE_EXIT_CODE=$?
        echo "⚠️ Provisioning completed with warnings (exit code: $ANSIBLE_EXIT_CODE)"
        echo "📋 Full log saved in container at: $ANSIBLE_LOG"
        echo "💡 Tips:"
        echo "   - Run with VM_DEBUG=true vm create to see detailed error output"
        echo "   - View the log: vm exec cat $ANSIBLE_LOG"
        echo "   - Or copy it: docker cp ${container_name}:$ANSIBLE_LOG ./ansible-error.log"
    fi

    # Ensure supervisor services are started
    echo "🚀 Starting services..."
    docker_run "exec" "$config" "$project_dir" bash -c "supervisorctl reread && supervisorctl update" || true

    # Clean up generated docker-compose.yml since containers are now running
    local compose_file
    compose_file="${project_dir}/docker-compose.yml"
    if [[ -f "$compose_file" ]]; then
        echo "✨ Cleanup complete"
        rm "$compose_file"
    fi

    echo "🎉 Environment ready!"

    # Automatically SSH into the container if auto-login is enabled
    if [[ "$auto_login" = "true" ]]; then
        echo "🌟 Entering development environment..."
        docker_ssh "$config" "" "."
    else
        echo "💡 Use 'vm ssh' to connect to the environment"
    fi
}

# Sync directory after VM exit - change to corresponding host directory if inside workspace
sync_directory_after_exit() {
    local config="$1"
    local project_dir="$2"
    local container_name
    container_name=$(get_project_container_name "$config")

    # Try to read the saved directory state from the container
    local vm_exit_dir=""
    if vm_exit_dir=$(docker_cmd exec "${container_name}" cat /tmp/vm-exit-directory 2>/dev/null || docker_cmd exec "${container_name}" cat ~/.vm-exit-directory 2>/dev/null); then
        # Remove any trailing whitespace/newlines
        vm_exit_dir=$(echo "$vm_exit_dir" | tr -d '\n\r')

        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG sync_directory_after_exit: vm_exit_dir='$vm_exit_dir'" >&2
            echo "DEBUG sync_directory_after_exit: project_dir='$project_dir'" >&2
        fi

        # If we have a relative path, construct the target host directory
        if [[ -n "$vm_exit_dir" ]]; then
            local target_host_dir="$project_dir/$vm_exit_dir"

            # Verify the target directory exists on the host
            if [[ -d "$target_host_dir" ]]; then
                if [[ "${VM_DEBUG:-}" = "true" ]]; then
                    echo "DEBUG sync_directory_after_exit: Target directory exists: '$target_host_dir'" >&2
                fi
                # Note: We can't change the parent shell's directory from here
                # The vm get-sync-directory command or shell wrapper should be used
            else
                if [[ "${VM_DEBUG:-}" = "true" ]]; then
                    echo "DEBUG sync_directory_after_exit: Target directory does not exist: '$target_host_dir'" >&2
                fi
            fi
        fi

        # Clean up the temporary files
        docker_cmd exec "${container_name}" rm -f /tmp/vm-exit-directory ~/.vm-exit-directory 2>/dev/null || true
    else
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG sync_directory_after_exit: No exit directory state found" >&2
        fi
    fi
}

docker_ssh() {
    local config="$1"
    local project_dir="$2"
    local relative_path="$3"
    shift 3

    # Get workspace path and user from config
    local workspace_path
    workspace_path=$(echo "$config" | yq -r '.project.workspace_path // "/workspace"')
    local project_user
    project_user=$(echo "$config" | yq -r '.vm.user // "developer"')
    local target_dir="${workspace_path}"

    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "DEBUG docker_ssh: relative_path='$relative_path'" >&2
        echo "DEBUG docker_ssh: workspace_path='$workspace_path'" >&2
    fi

    # If we have a relative path and it's not just ".", append it to workspace path
    if [[ -n "$relative_path" ]] && [[ "$relative_path" != "." ]]; then
        target_dir="${workspace_path}/${relative_path}"
    fi

    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "DEBUG docker_ssh: target_dir='$target_dir'" >&2
    fi

    # Handle -c flag specifically for command execution
    if [[ "$1" = "-c" ]] && [[ -n "$2" ]]; then
        # Run command non-interactively
        docker_run "exec" "$config" "" su - "$project_user" -c "cd '$target_dir' && source ~/.zshrc && $2"
    elif [[ $# -gt 0 ]]; then
        # Run with all arguments
        docker_run "exec" "$config" "" su - "$project_user" -c "cd '$target_dir' && source ~/.zshrc && zsh $*"
    else
        # Interactive mode - use a simple approach that works
        local container_name
        container_name=$(get_project_container_name "$config")

        # Run an interactive shell with the working directory set
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG docker_ssh: Executing: docker exec -it ${container_name} su - $project_user -c \"VM_TARGET_DIR='$target_dir' exec zsh\"" >&2
        fi

        # Use sudo for proper signal handling
        # This ensures proper Ctrl+C behavior (single tap interrupts, double tap to detach)
        docker_cmd exec -it "${container_name}" sudo -u "$project_user" sh -c "VM_TARGET_DIR='$target_dir' exec zsh"

        # After SSH session ends, check if we should change to a different directory
        sync_directory_after_exit "$config" "$project_dir"
    fi
}

docker_start() {
    local config="$1"
    local project_dir="$2"
    local relative_path="$3"
    local auto_login="$4"
    shift 4

    echo "🚀 Starting development environment..."

    # Get container name using shared function
    local container_name
    container_name=$(get_project_container_name "$config")

    # Check if container exists
    if ! docker_cmd inspect "${container_name}" >/dev/null 2>&1; then
        echo "❌ Container doesn't exist. Use 'vm create' to set up the environment first."
        return 1
    fi

    # Start the container directly (not using docker-compose)
    docker_cmd start "${container_name}" "$@"

    # Wait for container to be ready
    echo "⏳ Starting up..."
    local max_attempts=15
    local attempt=1
    while [[ $attempt -le $max_attempts ]]; do
        if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
            echo "✅ Environment ready!"
            break
        fi
        if [[ $attempt -eq $max_attempts ]]; then
            echo "❌ Environment startup failed"
            return 1
        fi
        sleep 1
        ((attempt++))
    done

    echo "🎉 Environment started!"

    # Automatically SSH into the container if auto-login is enabled
    if [[ "$auto_login" = "true" ]]; then
        echo "🌟 Entering development environment..."
        docker_ssh "$config" "$project_dir" "$relative_path"
    else
        echo "💡 Use 'vm ssh' to connect to the environment"
    fi
}

docker_halt() {
    local config="$1"
    local project_dir="$2"
    shift 2

    # Stop the container directly (not using docker-compose)
    local container_name
    container_name=$(get_project_container_name "$config")
    docker_cmd stop "${container_name}" "$@"
}

docker_destroy() {
    local config="$1"
    local project_dir="$2"
    shift 2

    # Get container name for user feedback
    local container_name
    container_name=$(get_project_container_name "$config")

    echo "🗑️ Destroying VM: ${container_name}"

    # Create a secure temporary file for the config
    TEMP_CONFIG_FILE=$(mktemp /tmp/vm-config.XXXXXX)
    # Ensure the temp file is removed when the script exits
    trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT

    # Generate docker-compose.yml temporarily for destroy operation
    echo "🧹 Preparing cleanup..."
    echo "$config" > "$TEMP_CONFIG_FILE"
    "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"

    # Run docker compose down with volumes
    docker_run "down" "$config" "$project_dir" -v "$@"

    # Clean up the generated docker-compose.yml after destroy
    local compose_file
    compose_file="${project_dir}/docker-compose.yml"
    if [[ -f "$compose_file" ]]; then
        echo "✨ Cleanup complete"
        rm "$compose_file"
    fi
}

docker_status() {
    local config="$1"
    local project_dir="$2"
    shift 2

    docker_run "ps" "$config" "$project_dir" "$@"
}

docker_reload() {
    local config="$1"
    local project_dir="$2"
    shift 2

    echo "🔄 Restarting VM..."

    # Stop the container with error handling
    if ! docker_halt "$config" "$project_dir"; then
        echo "❌ Failed to stop VM"
        return 1
    fi

    echo "✅ VM stopped successfully"

    # Regenerate docker-compose.yml to pick up config changes (npm links, etc.)
    echo "🔄 Updating configuration..."
    TEMP_CONFIG_FILE=$(mktemp /tmp/vm-config.XXXXXX)
    trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT
    echo "$config" > "$TEMP_CONFIG_FILE"
    "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"

    # Start the container with error handling
    # docker_start expects: config, project_dir, relative_path, auto_login, then any extra args
    if ! docker_start "$config" "$project_dir" "." "false" "$@"; then
        echo "❌ Failed to start VM"
        return 1
    fi

    echo "🎉 VM restarted successfully!"
}

docker_provision() {
    local config="$1"
    local project_dir="$2"
    shift 2

    echo "🔄 Rebuilding environment..."

    # Create a secure temporary file for the config
    TEMP_CONFIG_FILE=$(mktemp /tmp/vm-config.XXXXXX)
    # Ensure the temp file is removed when the script exits
    trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT

    # Generate fresh docker-compose.yml for provisioning
    echo "$config" > "$TEMP_CONFIG_FILE"
    "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"

    docker_run "compose" "$config" "$project_dir" build --no-cache
    docker_run "compose" "$config" "$project_dir" up -d "$@"

    # Clean up generated docker-compose.yml since containers are now running
    local compose_file
    compose_file="${project_dir}/docker-compose.yml"
    if [[ -f "$compose_file" ]]; then
        echo "✨ Cleanup complete"
        rm "$compose_file"
    fi
}

docker_logs() {
    local config="$1"
    local project_dir="$2"
    shift 2

    docker_run "logs" "$config" "$project_dir" "$@"
}

docker_exec() {
    local config="$1"
    shift

    docker_run "exec" "$config" "" "$@"
}

docker_kill() {
    echo "⏹️ Stopping environment..."
    local config="$1"
    local project_name
    project_name=$(get_project_name "$config")

    docker_cmd stop "${project_name}-dev" 2>/dev/null || true
    docker_cmd stop "${project_name}-postgres" 2>/dev/null || true
    docker_cmd stop "${project_name}-redis" 2>/dev/null || true
    docker_cmd stop "${project_name}-mongodb" 2>/dev/null || true

    echo "✅ All Docker containers stopped!"
}

# List all VM instances
vm_list() {
    echo "📋 VM Instances:"
    echo "=================="

    # Check if Docker is available
    if command -v docker &> /dev/null; then
        # First, show main project VMs
        echo ""
        echo "🐳 Project VMs:"
        echo "---------------"

        # Get all containers and filter for main project VMs
        local main_vms
        main_vms=$(docker_cmd ps -a --format "{{.Names}}\t{{.Status}}\t{{.CreatedAt}}" | awk '$1 ~ /-dev$/ && $1 !~ /^vmtemp/ {print}' 2>/dev/null || true)

        if [[ -n "$main_vms" ]]; then
            echo "NAME                    STATUS                       CREATED"
            echo "================================================================"
            echo "$main_vms" | while IFS=$'\t' read -r name status created; do
                printf "%-22s %-28s %s\n" "$name" "$status" "$created"
            done
        else
            echo "No project VMs found"
        fi

        # Show temp VMs separately
        echo ""
        echo "🚀 Temporary VMs:"
        echo "-----------------"

        # Check for temp VM from state file
        local TEMP_STATE_FILE="$HOME/.vm/temp-vm.state"
        if [[ -f "$TEMP_STATE_FILE" ]]; then
            local temp_container
            local created_at
            local project_dir
            temp_container=""
            created_at=""
            project_dir=""

            if command -v yq &> /dev/null; then
                temp_container=$(yq -r '.container_name // empty' "$TEMP_STATE_FILE" 2>/dev/null)
                created_at=$(yq -r '.created_at // empty' "$TEMP_STATE_FILE" 2>/dev/null)
                project_dir=$(yq -r '.project_dir // empty' "$TEMP_STATE_FILE" 2>/dev/null)
            else
                temp_container=$(grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
            fi

            if [[ -n "$temp_container" ]]; then
                # Check if container actually exists
                local temp_status
                temp_status=$(docker_cmd ps -a --filter "name=^${temp_container}$" --format "{{.Status}}" 2>/dev/null || echo "Not found")

                echo "NAME            TYPE    STATUS           MOUNTS                  CREATED"
                echo "======================================================================"

                # Get mounts in a more readable format
                local mounts=""
                if command -v yq &> /dev/null; then
                    # Check if new format (objects with source/target/permissions) exists
                    if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
                        # New format - extract source paths
                        mounts=$(yq -r '.mounts[].source' "$TEMP_STATE_FILE" 2>/dev/null | while read -r source; do
                            echo -n "$(basename "$source"), "
                        done | sed 's/, $//')
                    else
                        # Old format fallback (simple strings)
                        mounts=$(yq -r '.mounts[]?' "$TEMP_STATE_FILE" 2>/dev/null | while read -r mount; do
                            source_path=$(echo "$mount" | cut -d: -f1)
                            echo -n "$(basename "$source_path"), "
                        done | sed 's/, $//')
                    fi
                fi

                if [[ -z "$mounts" ]]; then
                    mounts="(unknown)"
                fi

                printf "%-14s  temp    %-16s %-22s %s\n" "$temp_container" "$temp_status" "$mounts" "$created_at"

                echo ""
                echo "💡 Commands:"
                echo "  vm temp ssh              # Connect to temp VM"
                echo "  vm temp status           # Show detailed status"
                echo "  vm temp destroy          # Destroy temp VM"
            else
                echo "No temp VMs found"
            fi
        else
            echo "No temp VMs found"
        fi

        # Show service containers
        echo ""
        echo "🔧 Service Containers:"
        echo "---------------------"

        local service_containers
        service_containers=$(docker_cmd ps -a --format "{{.Names}}\t{{.Status}}\t{{.CreatedAt}}" | awk '$1 ~ /postgres|redis|mongodb/ && $1 !~ /-dev$/ {print}' 2>/dev/null || true)

        if [[ -n "$service_containers" ]]; then
            echo "NAME                    STATUS                       CREATED"
            echo "================================================================"
            echo "$service_containers" | while IFS=$'\t' read -r name status created; do
                printf "%-22s %-28s %s\n" "$name" "$status" "$created"
            done
        else
            echo "No service containers found"
        fi
    fi

    # Check if Vagrant is available
    if command -v vagrant &> /dev/null; then
        echo ""
        echo "📦 Vagrant VMs:"
        echo "---------------"
        vagrant global-status 2>/dev/null || echo "No Vagrant VMs found"
    fi

    echo ""
}



# Migrate legacy vm.json to new vm.yaml format
vm_migrate() {
    # Default values
    local INPUT_FILE=""
    local OUTPUT_FILE="vm.yaml"
    local BACKUP_ENABLED="true"
    local DRY_RUN="false"
    local FORCE="false"
    local CHECK_MODE="false"

    # Parse options
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --input)
                shift
                INPUT_FILE="$1"
                shift
                ;;
            --output)
                shift
                OUTPUT_FILE="$1"
                shift
                ;;
            --backup)
                shift
                if [[ "$1" =~ ^(true|false)$ ]]; then
                    BACKUP_ENABLED="$1"
                    shift
                else
                    BACKUP_ENABLED="true"
                fi
                ;;
            --no-backup)
                BACKUP_ENABLED="false"
                shift
                ;;
            --dry-run)
                DRY_RUN="true"
                shift
                ;;
            --force)
                FORCE="true"
                shift
                ;;
            --check)
                CHECK_MODE="true"
                shift
                ;;
            -h|--help)
                echo "Usage: vm migrate [options]"
                echo ""
                echo "Options:"
                echo "  --input FILE      Input JSON file (default: vm.json in current directory)"
                echo "  --output FILE     Output YAML file (default: vm.yaml)"
                echo "  --backup [BOOL]   Create backup of input file (default: true)"
                echo "  --no-backup       Disable backup creation"
                echo "  --dry-run         Show what would be done without making changes"
                echo "  --force           Skip confirmation prompts"
                echo "  --check           Check if migration is needed without performing it"
                echo ""
                echo "Examples:"
                echo "  vm migrate                           # Migrate vm.json to vm.yaml"
                echo "  vm migrate --input config.json       # Migrate specific file"
                echo "  vm migrate --dry-run                 # Preview migration"
                echo "  vm migrate --check                   # Check if migration is needed"
                return 0
                ;;
            *)
                echo "❌ Unknown option: $1" >&2
                echo "Use 'vm migrate --help' for usage information" >&2
                return 1
                ;;
        esac
    done

    # Handle check mode
    if [[ "$CHECK_MODE" == "true" ]]; then
        if [[ -f "vm.json" ]] && [[ ! -f "vm.yaml" ]]; then
            echo "✅ Migration needed: vm.json exists but vm.yaml does not"
            echo "   Run 'vm migrate' to perform the migration"
            return 0
        elif [[ -f "vm.json" ]] && [[ -f "vm.yaml" ]]; then
            echo "⚠️  Both vm.json and vm.yaml exist"
            echo "   The vm.yaml file will be used by default"
            echo "   Consider removing vm.json if it's no longer needed"
            return 0
        elif [[ ! -f "vm.json" ]] && [[ -f "vm.yaml" ]]; then
            echo "✅ No migration needed: Already using vm.yaml"
            return 0
        else
            echo "❌ No configuration files found (neither vm.json nor vm.yaml)"
            return 1
        fi
    fi

    # Find source file if not specified
    if [[ -z "$INPUT_FILE" ]]; then
        if [[ -f "vm.json" ]]; then
            INPUT_FILE="vm.json"
        else
            echo "❌ No vm.json file found in current directory" >&2
            echo "   Use --input to specify a different file" >&2
            return 1
        fi
    fi

    # Verify input file exists
    if [[ ! -f "$INPUT_FILE" ]]; then
        echo "❌ Input file not found: $INPUT_FILE" >&2
        return 1
    fi

    # Check if output file already exists
    if [[ -f "$OUTPUT_FILE" ]] && [[ "$FORCE" != "true" ]] && [[ "$DRY_RUN" != "true" ]]; then
        echo "⚠️  Output file already exists: $OUTPUT_FILE"
        echo -n "Do you want to overwrite it? (y/N): "
        read -r response
        case "$response" in
            [yY]|[yY][eE][sS])
                ;;
            *)
                echo "❌ Migration cancelled"
                return 1
                ;;
        esac
    fi

    # Show migration plan if not forced
    if [[ "$FORCE" != "true" ]] && [[ "$DRY_RUN" != "true" ]]; then
        echo "📋 Migration Plan:"
        echo "  • Input:  $INPUT_FILE"
        echo "  • Output: $OUTPUT_FILE"
        if [[ "$BACKUP_ENABLED" == "true" ]]; then
            echo "  • Backup: ${INPUT_FILE}.bak"
        fi
        echo ""
        echo -n "Do you want to proceed? (y/N): "
        read -r response
        case "$response" in
            [yY]|[yY][eE][sS])
                ;;
            *)
                echo "❌ Migration cancelled"
                return 1
                ;;
        esac
    fi

    # Create backup if enabled
    if [[ "$BACKUP_ENABLED" == "true" ]] && [[ "$DRY_RUN" != "true" ]]; then
        echo "📦 Creating backup: ${INPUT_FILE}.bak"
        cp "$INPUT_FILE" "${INPUT_FILE}.bak"
    fi

    # Convert JSON to YAML
    echo "🔄 Converting JSON to YAML..."
    local YAML_CONTENT
    if ! YAML_CONTENT=$(yq -y . "$INPUT_FILE" 2>&1); then
        echo "❌ Failed to convert JSON to YAML:" >&2
        echo "   $YAML_CONTENT" >&2
        return 1
    fi

    # Remove $schema field (not needed for user configs)
    echo "🧹 Removing \$schema field..."
    YAML_CONTENT=$(echo "$YAML_CONTENT" | yq 'del(."$schema")' | yq -y .)

    # Add version field
    echo "📝 Adding version field..."
    YAML_CONTENT=$(echo "$YAML_CONTENT" | yq '. = {"version": "1.0"} + .' | yq -y .)

    # Handle dry run mode
    if [[ "$DRY_RUN" == "true" ]]; then
        echo ""
        echo "📄 Preview of generated $OUTPUT_FILE:"
        echo "======================================"
        echo "$YAML_CONTENT"
        echo "======================================"
        echo ""
        echo "✅ Dry run complete. No files were modified."
        return 0
    fi

    # Write the output file
    echo "$YAML_CONTENT" > "$OUTPUT_FILE"

    # Validate the new configuration
    echo "✅ Validating migrated configuration..."
    if ! "$SCRIPT_DIR/validate-config.sh" --validate "$OUTPUT_FILE"; then
        echo "❌ Migration completed but validation failed" >&2
        echo "   Please review and fix $OUTPUT_FILE manually" >&2
        return 1
    fi

    echo "✅ Migration completed successfully!"
    echo ""
    echo "📋 Next steps:"
    echo "  1. Review the migrated configuration: $OUTPUT_FILE"
    echo "  2. Test your VM with the new configuration"
    echo "  3. Once verified, you can remove the old file: $INPUT_FILE"

    # Ask about deleting the original file
    if [[ "$FORCE" != "true" ]]; then
        echo ""
        echo -n "Would you like to delete the original $INPUT_FILE now? (y/N): "
        read -r response
        case "$response" in
            [yY]|[yY][eE][sS])
                rm "$INPUT_FILE"
                echo "🗑️  Removed $INPUT_FILE"
                ;;
            *)
                echo "💡 Keeping $INPUT_FILE for now"
                ;;
        esac
    fi

    return 0
}

# Parse command line arguments manually for better control
CUSTOM_CONFIG=""
# DEBUG_MODE is deprecated, using VM_DEBUG instead
DRY_RUN="false"
AUTO_LOGIN="true"
ARGS=()

# Manual argument parsing - much simpler and more reliable than getopt
while [[ $# -gt 0 ]]; do
    case "$1" in
        -c|--config)
            shift
            # Check if next argument exists and is not a flag or command
            if [[ $# -eq 0 ]] || [[ "$1" =~ ^- ]] || [[ "$1" =~ ^(init|generate|validate|migrate|list|temp|create|start|stop|restart|ssh|destroy|status|provision|logs|exec|kill|help)$ ]]; then
                # No argument provided or next is a flag/command - use scan mode
                CUSTOM_CONFIG="__SCAN__"
            else
                # Argument provided - use it as config path
                if [[ -d "$1" ]]; then
                    CUSTOM_CONFIG="$1/vm.yaml"
                else
                    CUSTOM_CONFIG="$1"
                fi
                shift
            fi
            ;;
        -d|--debug)
            # DEBUG_MODE variable is deprecated, using VM_DEBUG instead
            export VM_DEBUG="true"
            shift
            ;;
        --dry-run)
            DRY_RUN="true"
            shift
            ;;
        --auto-login)
            shift
            # Check if next argument exists and is a boolean value
            if [[ $# -gt 0 ]] && [[ "$1" =~ ^(true|false)$ ]]; then
                AUTO_LOGIN="$1"
                shift
            else
                # Default to true if no argument provided
                AUTO_LOGIN="true"
            fi
            ;;
        --auto-login=*)
            # Handle --auto-login=true/false format
            AUTO_LOGIN="${1#*=}"
            if [[ ! "$AUTO_LOGIN" =~ ^(true|false)$ ]]; then
                echo "❌ Invalid value for --auto-login: $AUTO_LOGIN. Must be 'true' or 'false'." >&2
                exit 1
            fi
            shift
            ;;
        temp|tmp)
            # Special handling for temp/tmp command - pass all remaining args
            ARGS+=("$1")
            shift
            # Add all remaining arguments without parsing
            ARGS+=("$@")
            break
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        -*)
            echo "❌ Unknown option: $1" >&2
            show_usage
            exit 1
            ;;
        generate)
            # Special handling for generate command - pass all remaining args
            ARGS+=("$1")
            shift
            # Add all remaining arguments without parsing
            ARGS+=("$@")
            break
            ;;
        test)
            # Special handling for test command - pass all remaining args
            ARGS+=("$1")
            shift
            # Add all remaining arguments without parsing
            ARGS+=("$@")
            break
            ;;
        migrate)
            # Special handling for migrate command - pass all remaining args
            ARGS+=("$1")
            shift
            # Add all remaining arguments without parsing
            ARGS+=("$@")
            break
            ;;
        *)
            # Collect remaining arguments (command and its args)
            ARGS+=("$1")
            shift
            ;;
    esac
done

# Restore positional parameters to the command and its arguments
set -- "${ARGS[@]}"

# Handle special commands
case "${1:-}" in
    "init")
        echo "✨ Creating new project configuration..."
        # Use validate-config.sh with special init flag
        if [[ -n "$CUSTOM_CONFIG" ]] && [[ "$CUSTOM_CONFIG" != "__SCAN__" ]]; then
            "$SCRIPT_DIR/validate-config.sh" --init "$CUSTOM_CONFIG"
        else
            "$SCRIPT_DIR/validate-config.sh" --init
        fi
        ;;
    "generate")
        echo "⚙️ Generating configuration..."
        # Pass all remaining arguments to generate-config.sh
        shift
        "$SCRIPT_DIR/generate-config.sh" "$@"
        ;;
    "validate")
        echo "✅ Validating configuration..."
        # Validate configuration using the centralized config manager
        if [[ -n "$CUSTOM_CONFIG" ]]; then
            "$SCRIPT_DIR/validate-config.sh" --validate "$CUSTOM_CONFIG"
        else
            "$SCRIPT_DIR/validate-config.sh" --validate
        fi
        ;;
    "migrate")
        shift
        vm_migrate "$@"
        ;;
    "list")
        vm_list
        ;;
    "get-sync-directory")
        # Load config to get container info
        if ! CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR"); then
            exit 1
        fi

        PROVIDER=$(get_provider "$CONFIG")
        if [[ "$PROVIDER" != "docker" ]]; then
            echo "❌ Directory sync only supported for Docker provider" >&2
            exit 1
        fi

        # Determine project directory
        if [[ "$CUSTOM_CONFIG" = "__SCAN__" ]]; then
            PROJECT_DIR="$CURRENT_DIR"
        elif [[ -n "$CUSTOM_CONFIG" ]]; then
            FULL_CONFIG_PATH="$(cd "$CURRENT_DIR" && readlink -f "$CUSTOM_CONFIG")"
            PROJECT_DIR="$(dirname "$FULL_CONFIG_PATH")"
        else
            PROJECT_DIR="$CURRENT_DIR"
        fi

        # Get the sync directory and output absolute host path
        container_name=$(get_project_container_name "$CONFIG")
        if vm_exit_dir=$(docker_cmd exec "${container_name}" cat /tmp/vm-exit-directory 2>/dev/null || docker_cmd exec "${container_name}" cat ~/.vm-exit-directory 2>/dev/null); then
            vm_exit_dir=$(echo "$vm_exit_dir" | tr -d '\n\r')
            if [[ -n "$vm_exit_dir" ]]; then
                target_host_dir="$PROJECT_DIR/$vm_exit_dir"
                if [[ -d "$target_host_dir" ]]; then
                    echo "$target_host_dir"
                fi
            fi
        fi
        ;;
    "kill")
        # Load config to determine provider
        if ! CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR"); then
            echo "❌ Invalid configuration"
            exit 1
        fi

        PROVIDER=$(get_provider "$CONFIG")

        if [[ "$PROVIDER" = "docker" ]]; then
            docker_kill "$CONFIG"
        else
            kill_virtualbox
        fi
        ;;
    "temp"|"tmp")
        # Handle temp VM commands - delegate to vm-temp.sh module
        shift
        source "$SCRIPT_DIR/vm-temp.sh"
        handle_temp_command "$@"
        ;;
    "destroy")
        # Special handling for vm-temp
        if [[ "${2:-}" = "vm-temp" ]]; then
            echo "🗑️ Destroying temporary VM..."
            # Try both old and new container names for compatibility
            if docker_cmd rm -f "vmtemp-dev" >/dev/null 2>&1 || docker_cmd rm -f "vm-temp" >/dev/null 2>&1; then
                # Also clean up volumes
                docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true

                # Clean up temp project directory if it exists
                # Look for marker file in secure location (same logic as creation)
                if [[ -n "$XDG_RUNTIME_DIR" ]] && [[ -d "$XDG_RUNTIME_DIR/vm" ]]; then
                    MARKER_DIR="$XDG_RUNTIME_DIR/vm"
                elif [[ -d "$HOME/.local/state/vm" ]]; then
                    MARKER_DIR="$HOME/.local/state/vm"
                else
                    MARKER_DIR="/tmp"
                fi
                # Find marker file for this container
                TEMP_DIR_MARKER="$MARKER_DIR/.vmtemp-vmtemp-dev-marker"

                # Also check legacy location for backward compatibility
                if [[ ! -f "$TEMP_DIR_MARKER" ]] && [[ -f "/tmp/.vmtemp-project-dir" ]]; then
                    TEMP_DIR_MARKER="/tmp/.vmtemp-project-dir"
                    echo "⚠️  Warning: Using legacy marker file location"
                fi

                if [[ -f "$TEMP_DIR_MARKER" ]]; then
                    TEMP_PROJECT_DIR=$(cat "$TEMP_DIR_MARKER")
                    # Safety check: ensure it's a temp directory
                    if [[ -d "$TEMP_PROJECT_DIR" ]]; then
                        # Resolve the real path to prevent directory traversal attacks
                        # Use realpath to follow symlinks and get the canonical path
                        REAL_TEMP_DIR=$(realpath "$TEMP_PROJECT_DIR" 2>/dev/null)
                        if [[ "$REAL_TEMP_DIR" == /tmp/vm-temp-project-* ]]; then
                            # Additional safety: only remove if it contains symlinks (not real files)
                            if find "$REAL_TEMP_DIR" -maxdepth 1 -type f -print -quit | grep -q .; then
                                echo "⚠️  Warning: Temp directory contains real files, not cleaning up: $TEMP_PROJECT_DIR"
                                # Log security event
                                if command -v logger >/dev/null 2>&1; then
                                    logger -t vm-temp-security "WARN: Refused to delete temp directory with real files: $TEMP_PROJECT_DIR"
                                fi
                            else
                                rm -rf "$REAL_TEMP_DIR"
                                echo "🧹 Cleaned up temporary project directory"
                                # Log successful cleanup
                                if command -v logger >/dev/null 2>&1; then
                                    logger -t vm-temp "Cleaned up temp directory: $REAL_TEMP_DIR"
                                fi
                            fi
                        else
                            echo "⚠️  Warning: Invalid temp directory path (resolved to $REAL_TEMP_DIR), not cleaning up: $TEMP_PROJECT_DIR"
                            # Log security event
                            if command -v logger >/dev/null 2>&1; then
                                logger -t vm-temp-security "ALERT: Rejected suspicious temp path: $TEMP_PROJECT_DIR (resolved to: $REAL_TEMP_DIR)"
                            fi
                        fi
                    else
                        echo "⚠️  Warning: Temp directory not found: $TEMP_PROJECT_DIR"
                    fi
                    rm -f "$TEMP_DIR_MARKER"
                fi

                echo "✅ vm-temp destroyed successfully"

                # Clean up any stale marker files (older than 1 day)
                if [[ -d "$MARKER_DIR" ]]; then
                    find "$MARKER_DIR" -name ".vmtemp-*-marker" -type f -mtime +1 -delete 2>/dev/null || true
                fi
            else
                echo "❌ vm-temp not found or already destroyed"
            fi
            exit 0
        fi

        # If no VM name provided, load config from current directory and destroy
        if [[ $# -eq 1 ]]; then
            # Load and validate config
            if ! CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR"); then
                echo "❌ No vm.yaml configuration file found. Run \"vm init\" to create one."
                exit 1
            fi

            PROVIDER=$(get_provider "$CONFIG")

            # Determine project directory
            if [[ "$CUSTOM_CONFIG" = "__SCAN__" ]]; then
                # In scan mode, get the directory where config was found
                CONFIG_DIR=$(echo "$CONFIG" | yq -r '.__config_dir // empty' 2>/dev/null)
                if [[ -n "$CONFIG_DIR" ]]; then
                    PROJECT_DIR="$CONFIG_DIR"
                else
                    PROJECT_DIR="$CURRENT_DIR"
                fi
            elif [[ -n "$CUSTOM_CONFIG" ]]; then
                FULL_CONFIG_PATH="$(cd "$CURRENT_DIR" && readlink -f "$CUSTOM_CONFIG")"
                PROJECT_DIR="$(dirname "$FULL_CONFIG_PATH")"
            else
                PROJECT_DIR="$CURRENT_DIR"
            fi

            # Get container name for confirmation
            container_name=$(get_project_container_name "$CONFIG")

            echo "⚠️  About to destroy VM: ${container_name}"
            echo -n "Are you sure? This will destroy the VM and all its data. (y/N): "
            read -r response
            case "$response" in
                [yY]|[yY][eE][sS])
                    if [[ "$PROVIDER" = "docker" ]]; then
                        docker_destroy "$CONFIG" "$PROJECT_DIR"
                    else
                        VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant destroy -f
                    fi
                    ;;
                *)
                    echo "❌ Destroy cancelled."
                    exit 1
                    ;;
            esac
            exit 0
        fi
        # Fall through to default case for destroy with arguments
        ;&
    "help"|"-h"|"--help"|"")
        show_usage
        ;;
    *)
        # Load and validate config (discovery handled by validate-config.js)
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG main: CUSTOM_CONFIG='$CUSTOM_CONFIG'" >&2
        fi
        if ! CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR"); then
            echo "❌ Invalid configuration"
            exit 1
        fi

        PROVIDER=$(get_provider "$CONFIG")

        # Determine project directory and config path
        if [[ "$CUSTOM_CONFIG" = "__SCAN__" ]]; then
            # Scan mode: project dir is where user ran the command
            PROJECT_DIR="$CURRENT_DIR"
            FULL_CONFIG_PATH=""
        elif [[ -n "$CUSTOM_CONFIG" ]]; then
            # If using custom config, project dir is where the config file is located
            # Resolve the path from the original directory where user ran the command
            FULL_CONFIG_PATH="$(cd "$CURRENT_DIR" && readlink -f "$CUSTOM_CONFIG")"
            PROJECT_DIR="$(dirname "$FULL_CONFIG_PATH")"
        else
            # Default: current directory, no explicit config path (uses discovery)
            PROJECT_DIR="$CURRENT_DIR"
            FULL_CONFIG_PATH=""
        fi

        # --- INSERT THIS VALIDATION LOGIC ---
        # Security: Validate that PROJECT_DIR is a legitimate project directory
        # and not a sensitive system path.
        if [[ -z "$PROJECT_DIR" ]] || [[ "$PROJECT_DIR" == "/" ]] || [[ "$PROJECT_DIR" == "/etc" ]] || [[ "$PROJECT_DIR" == "/usr" ]] || [[ "$PROJECT_DIR" == "/var" ]] || [[ "$PROJECT_DIR" == "/bin" ]] || [[ "$PROJECT_DIR" == "/sbin" ]]; then
            echo "❌ Error: Refusing to operate on critical system directory '$PROJECT_DIR'." >&2
            exit 1
        fi

        # Check for a project marker to prevent running in unintended locations.
        if [[ ! -d "$PROJECT_DIR/.git" ]] && [[ ! -f "$PROJECT_DIR/vm.yaml" ]] && [[ ! -f "$PROJECT_DIR/vm.schema.yaml" ]]; then
            echo "❌ Error: The specified directory '$PROJECT_DIR' does not appear to be a valid project root." >&2
            echo "   (Missing a .git directory, vm.yaml, or vm.schema.yaml file to act as a safeguard)." >&2
            exit 1
        fi
        # --- END OF VALIDATION LOGIC ---

        echo "🐳 Using provider: $PROVIDER"

        # Show dry run information if enabled
        if [[ "$DRY_RUN" = "true" ]]; then
            echo ""
            echo "🔍 DRY RUN MODE - showing what would be executed:"
            echo "   Project directory: $PROJECT_DIR"
            echo "   Provider: $PROVIDER"
            echo "   Command: $1"
            echo "   Arguments: $*"
            if [[ "$CUSTOM_CONFIG" = "__SCAN__" ]]; then
                echo "   Config mode: Scanning up directory tree"
            elif [[ -n "$CUSTOM_CONFIG" ]]; then
                echo "   Config mode: Explicit config ($CUSTOM_CONFIG)"
            else
                echo "   Config mode: Default discovery"
            fi
            echo ""
            echo "🚫 Dry run complete - no commands were executed"
            exit 0
        fi

        # Route command to appropriate provider
        COMMAND="$1"
        shift

        if [[ "$PROVIDER" = "docker" ]]; then
            case "$COMMAND" in
                "create")
                    # Check if VM already exists and confirm before recreating
                    container_name=$(get_project_container_name "$CONFIG")

                    if docker_cmd inspect "${container_name}" >/dev/null 2>&1; then
                        echo "⚠️  VM '${container_name}' already exists."
                        echo -n "Are you sure you want to recreate it? This will destroy the existing VM and all its data. (y/N): "
                        read -r response
                        case "$response" in
                            [yY]|[yY][eE][sS])
                                echo "🗑️  Destroying existing VM first..."
                                docker_destroy "$CONFIG" "$PROJECT_DIR"
                                ;;
                            *)
                                echo "❌ VM creation cancelled."
                                exit 1
                                ;;
                        esac
                    fi

                    docker_up "$CONFIG" "$PROJECT_DIR" "$AUTO_LOGIN" "$@"
                    ;;
                "start")
                    # Calculate relative path for start (same logic as SSH command)
                    if [[ "$CUSTOM_CONFIG" = "__SCAN__" ]]; then
                        # In scan mode, we need to figure out where we are relative to the found config
                        # Get the directory where vm.yaml was found from validate-config output
                        CONFIG_DIR=$(echo "$CONFIG" | yq -r '.__config_dir // empty' 2>/dev/null)
                        if [[ "${VM_DEBUG:-}" = "true" ]]; then
                            echo "DEBUG start: CUSTOM_CONFIG='$CUSTOM_CONFIG'" >&2
                            echo "DEBUG start: CONFIG_DIR='$CONFIG_DIR'" >&2
                            echo "DEBUG start: CURRENT_DIR='$CURRENT_DIR'" >&2
                        fi
                        if [[ -n "$CONFIG_DIR" ]] && [[ "$CONFIG_DIR" != "$CURRENT_DIR" ]]; then
                            # Calculate path from config dir to current dir
                            RELATIVE_PATH=$(realpath --relative-to="$CONFIG_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
                        else
                            RELATIVE_PATH="."
                        fi
                    else
                        # Normal mode: relative path from project dir to current dir
                        RELATIVE_PATH=$(realpath --relative-to="$PROJECT_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
                    fi
                    if [[ "${VM_DEBUG:-}" = "true" ]]; then
                        echo "DEBUG start: RELATIVE_PATH='$RELATIVE_PATH'" >&2
                    fi
                    docker_start "$CONFIG" "$PROJECT_DIR" "$RELATIVE_PATH" "$AUTO_LOGIN" "$@"
                    ;;
                "stop")
                    docker_halt "$CONFIG" "$PROJECT_DIR" "$@"
                    ;;
                "restart")
                    docker_reload "$CONFIG" "$PROJECT_DIR" "$@"
                    ;;
                "ssh")
                    # Calculate relative path for SSH
                    if [[ "$CUSTOM_CONFIG" = "__SCAN__" ]]; then
                        # In scan mode, we need to figure out where we are relative to the found config
                        # Get the directory where vm.yaml was found from validate-config output
                        CONFIG_DIR=$(echo "$CONFIG" | yq -r '.__config_dir // empty' 2>/dev/null)
                        if [[ -n "$CONFIG_DIR" ]] && [[ "$CONFIG_DIR" != "$CURRENT_DIR" ]]; then
                            # Calculate path from config dir to current dir
                            RELATIVE_PATH=$(realpath --relative-to="$CONFIG_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
                        else
                            RELATIVE_PATH="."
                        fi
                    else
                        # Normal mode: relative path from project dir to current dir
                        RELATIVE_PATH=$(realpath --relative-to="$PROJECT_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
                    fi

                    if [[ "${VM_DEBUG:-}" = "true" ]]; then
                        echo "DEBUG ssh: CURRENT_DIR='$CURRENT_DIR'" >&2
                        echo "DEBUG ssh: PROJECT_DIR='$PROJECT_DIR'" >&2
                        echo "DEBUG ssh: CUSTOM_CONFIG='$CUSTOM_CONFIG'" >&2
                        echo "DEBUG ssh: CONFIG_DIR='$CONFIG_DIR'" >&2
                        echo "DEBUG ssh: RELATIVE_PATH='$RELATIVE_PATH'" >&2
                    fi

                    # Get container name for connection message
                    container_name=$(get_project_container_name "$CONFIG")
                    echo "🎯 Connected to $container_name"

                    docker_ssh "$CONFIG" "$PROJECT_DIR" "$RELATIVE_PATH" "$@"
                    ;;
                "destroy")
                    docker_destroy "$CONFIG" "$PROJECT_DIR" "$@"
                    ;;
                "status")
                    docker_status "$CONFIG" "$PROJECT_DIR" "$@"
                    ;;
                "provision")
                    docker_provision "$CONFIG" "$PROJECT_DIR" "$@"
                    ;;
                "logs")
                    docker_logs "$CONFIG" "$PROJECT_DIR" "$@"
                    ;;
                "exec")
                    docker_exec "$CONFIG" "$@"
                    ;;
                "test")
                    # Run tests using test.sh
                    "$SCRIPT_DIR/test.sh" "$@"
                    ;;
                *)
                    echo "❌ Unknown command for Docker provider: $COMMAND"
                    exit 1
                    ;;
            esac
        else
            # Vagrant provider
            case "$COMMAND" in
                "create")
                    # Check if VM already exists and confirm before recreating
                    if VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant status default 2>/dev/null | grep -q "running\|poweroff\|saved"; then
                        echo "⚠️  Vagrant VM already exists."
                        echo -n "Are you sure you want to recreate it? This will destroy the existing VM and all its data. (y/N): "
                        read -r response
                        case "$response" in
                            [yY]|[yY][eE][sS])
                                echo "🗑️  Destroying existing VM first..."
                                VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant destroy -f
                                ;;
                            *)
                                echo "❌ VM creation cancelled."
                                exit 1
                                ;;
                        esac
                    fi

                    # Start VM
                    if [[ -n "$FULL_CONFIG_PATH" ]]; then
                        VM_PROJECT_DIR="$PROJECT_DIR" VM_CONFIG="$FULL_CONFIG_PATH" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
                    else
                        VM_PROJECT_DIR="$PROJECT_DIR" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
                    fi

                    # Auto-SSH if enabled
                    if [[ "$AUTO_LOGIN" = "true" ]]; then
                        echo "🔗 Connecting to VM..."
                        VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh
                    else
                        echo "💡 Use 'vm ssh' to connect to the VM"
                    fi
                    ;;
                "ssh")
                    # SSH into VM with relative path support
                    # Calculate relative path (similar to Docker SSH logic)
                    if [[ "$CUSTOM_CONFIG" = "__SCAN__" ]]; then
                        # In scan mode, figure out where we are relative to the found config
                        CONFIG_DIR=$(echo "$CONFIG" | yq -r '.__config_dir // empty' 2>/dev/null)
                        if [[ -n "$CONFIG_DIR" ]] && [[ "$CONFIG_DIR" != "$CURRENT_DIR" ]]; then
                            RELATIVE_PATH=$(realpath --relative-to="$CONFIG_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
                        else
                            RELATIVE_PATH="."
                        fi
                    else
                        # Normal mode: relative path from project dir to current dir
                        RELATIVE_PATH=$(realpath --relative-to="$PROJECT_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
                    fi

                    # Get workspace path from config
                    WORKSPACE_PATH=$(echo "$CONFIG" | yq -r '.project.workspace_path // "/workspace"')

                    if [[ "$RELATIVE_PATH" != "." ]]; then
                        TARGET_DIR="${WORKSPACE_PATH}/${RELATIVE_PATH}"
                        VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "cd '$TARGET_DIR' && exec /bin/zsh"
                    else
                        VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh
                    fi
                    ;;
                "start")
                    # Start existing VM (Vagrant equivalent of resume)
                    VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant resume "$@" || VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
                    ;;
                "stop")
                    # Stop VM but keep data
                    VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant halt "$@"
                    ;;
                "restart")
                    # Restart VM without reprovisioning
                    VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant halt "$@"
                    VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant resume "$@" || VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
                    ;;
                "exec")
                    # Execute command in Vagrant VM
                    VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "$@"
                    ;;
                "logs")
                    # Show service logs in Vagrant VM
                    echo "📋 Showing service logs (Ctrl+C to stop)..."
                    VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "sudo journalctl -u postgresql -u redis-server -u mongod -f"
                    ;;
                "test")
                    # Run tests using test.sh
                    "$SCRIPT_DIR/test.sh" "$@"
                    ;;
                *)
                    # Pass through to vagrant command
                    if [[ -n "$FULL_CONFIG_PATH" ]]; then
                        VM_PROJECT_DIR="$PROJECT_DIR" VM_CONFIG="$FULL_CONFIG_PATH" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant "$COMMAND" "$@"
                    else
                        VM_PROJECT_DIR="$PROJECT_DIR" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant "$COMMAND" "$@"
                    fi
                    ;;
            esac
        fi
        ;;
esac
