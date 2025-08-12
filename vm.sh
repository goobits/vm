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

# Global variables - defined at script level for use across functions
# SCRIPT_DIR: Directory containing this script and shared utilities
# CURRENT_DIR: User's working directory when script was invoked  
# Used by provider interface and config processing
CURRENT_DIR="$(pwd)"

# Source shared utilities
source "$SCRIPT_DIR/shared/npm-utils.sh"
source "$SCRIPT_DIR/shared/docker-utils.sh"
source "$SCRIPT_DIR/shared/temporary-file-utils.sh"
source "$SCRIPT_DIR/shared/mount-utils.sh"
source "$SCRIPT_DIR/shared/security-utils.sh"
source "$SCRIPT_DIR/shared/config-processor.sh"
source "$SCRIPT_DIR/shared/provider-interface.sh"
source "$SCRIPT_DIR/shared/project-detector.sh"
source "$SCRIPT_DIR/lib/progress-reporter.sh"
source "$SCRIPT_DIR/lib/docker-compose-progress.sh"

# Set up proper cleanup handlers for temporary files
setup_temp_file_handlers

# Export environment variables needed by provider interface
export CURRENT_DIR
export CUSTOM_CONFIG
export FULL_CONFIG_PATH

# Mount validation functions moved to shared/mount-utils.sh

# Mount validation functions are now available from shared/mount-utils.sh
# All mount processing functions (validate_mount_security, parse_mount_string, etc.) 
# have been moved to shared/mount-utils.sh and are automatically sourced above.


# All mount processing functions have been moved to shared/mount-utils.sh
# Functions available: detect_comma_in_paths, parse_mount_permissions, construct_mount_argument,
# process_single_mount, parse_mount_string, validate_mount_security, validate_mount_security_atomic


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

    echo "Usage: $0 [--config [PATH]] [--debug] [--dry-run] [--auto-login [true|false]] [--no-preset] [--preset NAME] [--interactive] [command] [args...]"
    echo ""
    echo "Options:"
    echo "  --config [PATH]      Use specific vm.yaml file, or scan up directory tree if no path given"
    echo "  --debug              Enable debug output"
    echo "  --dry-run            Show what would be executed without actually running it"
    echo "  --auto-login [BOOL]  Automatically SSH into VM after create/start (default: true)"
    echo "  --no-preset          Disable automatic preset detection and application"
    echo "  --preset NAME        Force a specific preset (base, nodejs, python, etc.)"
    echo "  --interactive        Enable interactive mode for preset selection"
    echo ""
    echo "Commands:"
    echo "  init                  Initialize a new vm.yaml configuration file"
    echo "  generate              Generate vm.yaml by composing services"
    echo "  validate              Validate VM configuration"
    echo "  preset <subcommand>   Manage configuration presets"
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

# Function to load and validate config (now uses shared config processor)
load_config() {
    local config_path="$1"
    local original_dir="$2"

    # Use enhanced config loading with preset support if presets are enabled
    if [[ "${VM_USE_PRESETS:-true}" = "true" ]]; then
        load_config_with_presets "$config_path" "$original_dir"
    else
        # Use standard config loading without presets
        load_and_merge_config "$config_path" "$original_dir"
    fi
}


# Helper functions for interactive preset selection
get_available_presets() {
    local presets_dir="$SCRIPT_DIR/configs/presets"
    local available_presets=()
    
    if [[ -d "$presets_dir" ]]; then
        for preset_file in "$presets_dir"/*.yaml; do
            if [[ -f "$preset_file" ]]; then
                local preset_name
                preset_name=$(basename "$preset_file" .yaml)
                # Skip base preset as it's always included
                if [[ "$preset_name" != "base" ]]; then
                    available_presets+=("$preset_name")
                fi
            fi
        done
    fi
    
    printf '%s\n' "${available_presets[@]}" | sort
}

# Display preset file contents for user review
show_preset_details() {
    local preset_name="$1"
    local preset_file="$SCRIPT_DIR/configs/presets/${preset_name}.yaml"
    
    if [[ -f "$preset_file" ]]; then
        echo ""
        echo "📄 Contents of $preset_name preset:"
        echo "────────────────────────────────────"
        cat "$preset_file"
        echo "────────────────────────────────────"
        echo ""
    else
        echo "❌ Preset file not found: $preset_name"
    fi
}

# Check if preset is in array (utility function)
preset_in_array() {
    local preset="$1"
    shift
    local arr=("$@")
    
    for item in "${arr[@]}"; do
        if [[ "$item" == "$preset" ]]; then
            return 0
        fi
    done
    return 1
}

# Interactive preset selection menu
# Provides a user-friendly interface for customizing preset selection.
# Allows users to add/remove presets, view details, and finalize their selection.
# This improves user experience when the automatic detection needs manual refinement.
# Args: initial_presets (array passed by reference)
# Modifies the array with user's final selection
interactive_preset_selection() {
    local -n initial_presets_ref="$1"
    
    echo ""
    echo "🔍 Detected presets: ${initial_presets_ref[*]}"
    echo ""
    
    local menu_running=true
    local current_presets=("${initial_presets_ref[@]}")
    
    while [[ "$menu_running" == true ]]; do
        echo "Interactive Preset Selection:"
        echo "Current selection: ${current_presets[*]:-none}"
        echo ""
        
        local PS3="Choice: "
        local options=(
            "Use current selection and proceed"
            "Add additional preset"
            "Remove preset"
            "View preset details"
            "Reset to detected presets"
        )
        
        select choice in "${options[@]}"; do
            case $REPLY in
                1)
                    menu_running=false
                    break
                    ;;
                2)
                    # Add additional preset
                    echo ""
                    echo "Available presets to add:"
                    local available_presets
                    mapfile -t available_presets < <(get_available_presets)
                    
                    if [[ ${#available_presets[@]} -eq 0 ]]; then
                        echo "❌ No additional presets available"
                        break
                    fi
                    
                    local add_options=("${available_presets[@]}" "Cancel")
                    local PS3="Select preset to add: "
                    
                    select add_preset in "${add_options[@]}"; do
                        if [[ "$add_preset" == "Cancel" ]]; then
                            break
                        elif [[ -n "$add_preset" ]]; then
                            if preset_in_array "$add_preset" "${current_presets[@]}"; then
                                echo "⚠️  $add_preset is already selected"
                            else
                                current_presets+=("$add_preset")
                                echo "✅ Added $add_preset to selection"
                            fi
                            break
                        else
                            echo "❌ Invalid selection. Please try again."
                        fi
                    done
                    break
                    ;;
                3)
                    # Remove preset
                    if [[ ${#current_presets[@]} -eq 0 ]]; then
                        echo "❌ No presets selected to remove"
                        break
                    fi
                    
                    echo ""
                    echo "Current presets:"
                    local remove_options=("${current_presets[@]}" "Cancel")
                    local PS3="Select preset to remove: "
                    
                    select remove_preset in "${remove_options[@]}"; do
                        if [[ "$remove_preset" == "Cancel" ]]; then
                            break
                        elif [[ -n "$remove_preset" ]]; then
                            # Remove preset from array
                            local new_presets=()
                            for preset in "${current_presets[@]}"; do
                                if [[ "$preset" != "$remove_preset" ]]; then
                                    new_presets+=("$preset")
                                fi
                            done
                            current_presets=("${new_presets[@]}")
                            echo "✅ Removed $remove_preset from selection"
                            break
                        else
                            echo "❌ Invalid selection. Please try again."
                        fi
                    done
                    break
                    ;;
                4)
                    # View preset details
                    echo ""
                    echo "Available presets:"
                    local available_presets
                    mapfile -t available_presets < <(get_available_presets)
                    
                    if [[ ${#available_presets[@]} -eq 0 ]]; then
                        echo "❌ No presets available to view"
                        break
                    fi
                    
                    local view_options=("${available_presets[@]}" "Cancel")
                    local PS3="Select preset to view: "
                    
                    select view_preset in "${view_options[@]}"; do
                        if [[ "$view_preset" == "Cancel" ]]; then
                            break
                        elif [[ -n "$view_preset" ]]; then
                            show_preset_details "$view_preset"
                            echo "Press Enter to continue..."
                            read -r
                            break
                        else
                            echo "❌ Invalid selection. Please try again."
                        fi
                    done
                    break
                    ;;
                5)
                    # Reset to detected presets
                    current_presets=("${initial_presets_ref[@]}")
                    echo "✅ Reset to detected presets: ${current_presets[*]}"
                    break
                    ;;
                *)
                    echo "❌ Invalid selection. Please choose 1-5."
                    ;;
            esac
        done
        echo ""
    done
    
    # Update the reference array with user selection
    initial_presets_ref=("${current_presets[@]}")
    
    if [[ ${#initial_presets_ref[@]} -eq 0 ]]; then
        echo "⚠️  No presets selected. Using base preset only."
    else
        echo "✅ Final preset selection: ${initial_presets_ref[*]}"
    fi
    echo ""
}

# Apply smart preset when no vm.yaml exists
apply_smart_preset() {
    local project_dir="$1"
    
    # Use forced preset if specified, otherwise detect
    local detected_type
    local preset_types=()
    
    if [[ -n "${VM_FORCED_PRESET:-}" ]]; then
        detected_type="$VM_FORCED_PRESET"
        preset_types=("$VM_FORCED_PRESET")
        echo "🎯 Using forced preset: $VM_FORCED_PRESET"
    else
        # Detect project type
        detected_type=$(detect_project_type "$project_dir")
        echo "🔍 Detecting project type..."
        echo "✅ Detected: $detected_type"
        
        # Parse multi-preset strings or single preset
        if [[ "$detected_type" == multi:* ]]; then
            # Extract types from multi:type1 type2 type3 format
            local multi_types="${detected_type#multi:}"
            # Split by spaces into array
            IFS=' ' read -ra preset_types <<< "$multi_types"
        else
            # Single preset type
            preset_types=("$detected_type")
        fi
        
        # Interactive preset selection when VM_INTERACTIVE="true"
        if [[ "${VM_INTERACTIVE:-false}" == "true" ]]; then
            interactive_preset_selection preset_types
        fi
    fi
    
    # Define base preset file
    local base_preset_file="$SCRIPT_DIR/configs/presets/base.yaml"
    
    # Check if base preset exists
    if [[ ! -f "$base_preset_file" ]]; then
        echo "❌ Base preset file not found: $base_preset_file" >&2
        return 1
    fi
    
    # Start with the base preset (foundation layer)
    local merged_config
    merged_config=$(cat "$base_preset_file")
    echo "📦 Applying base development preset..."
    
    # Determine merge order strategy for multi-preset scenarios
    local ordered_presets=()
    local framework_presets=()
    local environment_presets=()
    
    # Categorize presets for proper merge order
    for preset_type in "${preset_types[@]}"; do
        case "$preset_type" in
            react|vue|next|angular|django|flask|rails|nodejs|python|ruby|php|rust|go)
                framework_presets+=("$preset_type")
                ;;
            docker|kubernetes)
                environment_presets+=("$preset_type")
                ;;
            generic)
                # Skip generic as base already provides foundation
                ;;
            *)
                # Unknown preset types go to framework layer
                framework_presets+=("$preset_type")
                ;;
        esac
    done
    
    # Merge framework presets first (after base)
    for preset_type in "${framework_presets[@]}"; do
        local preset_file="$SCRIPT_DIR/configs/presets/${preset_type}.yaml"
        if [[ -f "$preset_file" ]]; then
            echo "📦 Applying $preset_type development preset..."
            local temp_preset
            temp_preset=$(cat "$preset_file")
            merged_config=$(echo -e "$merged_config\n---\n$temp_preset")
        else
            echo "⚠️  Warning: Preset file not found for '$preset_type', skipping..."
        fi
    done
    
    # Merge environment presets last (override framework settings)
    for preset_type in "${environment_presets[@]}"; do
        local preset_file="$SCRIPT_DIR/configs/presets/${preset_type}.yaml"
        if [[ -f "$preset_file" ]]; then
            echo "📦 Applying $preset_type environment preset..."
            local temp_preset
            temp_preset=$(cat "$preset_file")
            merged_config=$(echo -e "$merged_config\n---\n$temp_preset")
        else
            echo "⚠️  Warning: Preset file not found for '$preset_type', skipping..."
        fi
    done
    
    # Log final merge summary
    local applied_count=$((${#framework_presets[@]} + ${#environment_presets[@]}))
    if [[ $applied_count -eq 0 ]]; then
        echo "📦 Applied base preset only (generic development environment)"
    elif [[ $applied_count -eq 1 ]]; then
        echo "📦 Applied base + 1 additional preset"
    else
        echo "📦 Applied base + $applied_count additional presets"
        echo "   Merge order: base → frameworks [${framework_presets[*]}] → environments [${environment_presets[*]}]"
    fi
    
    # Extract schema defaults and merge with preset
    local schema_defaults
    if ! schema_defaults=$("$SCRIPT_DIR/validate-config.sh" --extract-defaults "$SCRIPT_DIR/vm.schema.yaml" 2>&1); then
        echo "❌ Failed to extract schema defaults" >&2
        return 1
    fi
    
    # Merge schema defaults with preset (schema defaults first, then preset overrides)
    local final_config
    final_config=$(echo -e "$schema_defaults\n---\n$merged_config")
    
    echo "💡 You can customize this by creating a vm.yaml file"
    echo ""
    
    # Return the merged configuration
    echo "$final_config"
}

# Get provider from config (now uses shared config processor)
get_provider() {
    local config="$1"
    get_config_provider "$config"
}

# Extract project name from config (now uses shared config processor)
get_project_name() {
    local config="$1"
    get_config_project_name "$config"
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

# Shared function for container startup and readiness check
docker_startup_and_wait() {
    local config="$1"
    local project_dir="$2"
    local auto_login="$3"
    shift 3

    echo "🚀 Starting containers..."
    if ! docker_run "compose" "$config" "$project_dir" up -d "$@"; then
        local startup_error_code=$?
        echo "❌ Container startup failed (exit code: $startup_error_code)"
        echo "🧹 Performing startup rollback - stopping any running containers..."
        
        # Enhanced rollback with verification
        if docker_run "compose" "$config" "$project_dir" down; then
            echo "✅ Containers stopped successfully during rollback"
        else
            echo "⚠️ Warning: Rollback may be incomplete - some containers might still be running"
            echo "💡 Check with 'docker ps' and manually stop containers if needed"
        fi
        
        return $startup_error_code
    fi

    # Get container name using shared function
    local container_name
    container_name=$(get_project_container_name "$config")
    
    # Wait for container to be responsive with timeout
    local max_attempts=30
    local attempt=1
    local container_ready=false
    
    echo "⏳ Waiting for environment to be ready..."
    while [[ $attempt -le $max_attempts ]]; do
        if [[ $attempt -gt 1 ]]; then
            sleep 1
        fi
        
        # Check if container is still running first
        if ! docker_cmd ps --format "table {{.Names}}" | grep -q "^${container_name}$"; then
            echo "❌ Container stopped unexpectedly during startup"
            echo "💡 Check logs with: vm logs"
            return 1
        fi
        
        # Test if container is responsive
        if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
            echo "✅ Environment ready!"
            container_ready=true
            break
        fi
        
        if [[ $attempt -eq $max_attempts ]]; then
            echo "❌ Environment startup failed - container not responding"
            echo "💡 Container is running but not accepting exec commands"
            echo "💡 This may indicate:"
            echo "   - Container processes still starting"
            echo "   - Security policies blocking exec" 
            echo "   - Try: docker logs ${container_name}"
            return 1
        fi
        
        ((attempt++))
    done
    
    if [[ "$container_ready" != "true" ]]; then
        echo "❌ Environment startup failed - container not ready"
        return 1
    fi

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
        docker_ssh "$config" "$project_dir" "."
    else
        echo "💡 Use 'vm ssh' to connect to the environment"
    fi
}

docker_up() {
    local config="$1"
    local project_dir="$2"
    local auto_login="$3"
    shift 3

    # Get container name for display
    local container_name
    container_name=$(get_project_container_name "$config")
    
    # Initialize progress reporter
    progress_init "VM Operation" "$container_name"
    
    # CREATE PHASE
    progress_phase "🚀" "CREATE PHASE"
    
    # Show provider
    progress_task "🐳 Provider: docker"
    progress_done
    
    # Create a secure temporary file for the config
    TEMP_CONFIG_FILE=$(create_temp_file "vm-config.XXXXXX")

    # Generate docker-compose.yml
    echo "$config" > "$TEMP_CONFIG_FILE"
    "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"

    # Build container phase
    progress_phase "🔨" "Building container..." "├─"
    
    # Track build start time
    local build_start=$(date +%s)
    
    # Run docker compose build with progress tracking
    local build_output=""
    local build_success=true
    
    # Capture build output while showing progress
    while IFS= read -r line; do
        build_output+="$line"$'\n'
        case "$line" in
            *"[internal]"*)
                progress_subtask_done "Loading build definitions"
                ;;
            *"FROM"*"ubuntu"*)
                progress_subtask_done "Using Ubuntu 24.04 base"
                ;;
            "#"*"["*"/"*"]"*)
                # Extract step info
                if [[ "$line" =~ \[([0-9]+)/([0-9]+)\] ]]; then
                    current="${BASH_REMATCH[1]}"
                    total="${BASH_REMATCH[2]}"
                    progress_subtask_done "Building layers ($current/$total)"
                fi
                ;;
            *"naming to"*)
                progress_subtask_done "Image built"
                ;;
        esac
    done < <(docker_run "compose" "$config" "$project_dir" build 2>&1 || echo "BUILD_FAILED:$?")
    
    # Check if build failed
    if [[ "$build_output" =~ BUILD_FAILED:([0-9]+) ]]; then
        local build_error_code="${BASH_REMATCH[1]}"
        progress_fail "Build failed (exit code: $build_error_code)"
        
        progress_phase "🧹" "Cleanup" "├─"
        progress_task "Removing build artifacts"
        
        if docker_run "compose" "$config" "$project_dir" down --remove-orphans; then
            progress_done
        else
            progress_fail "Some artifacts may remain"
        fi
        
        # Clean up temp config file on build failure
        if [[ -f "$TEMP_CONFIG_FILE" ]]; then
            rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
        fi
        
        progress_phase_done
        progress_complete "Build failed"
        return $build_error_code
    fi
    
    progress_phase_done "Build complete"
    
    # Start services phase
    progress_phase "📦" "Starting services..." "├─"
    
    # Run docker compose up with progress tracking
    local startup_output=""
    while IFS= read -r line; do
        startup_output+="$line"$'\n'
        case "$line" in
            *"Network"*"Created"*)
                progress_subtask_done "Network created"
                ;;
            *"Volume"*"Created"*)
                # Count volumes
                volume_count=$(echo "$line" | grep -o "Volume" | wc -l)
                progress_subtask_done "Volumes created ($volume_count)"
                ;;
            *"Container"*"Started"*)
                progress_subtask_done "Container started"
                ;;
        esac
    done < <(docker_run "compose" "$config" "$project_dir" up -d "$@" 2>&1 || echo "STARTUP_FAILED:$?")
    
    # Check if startup failed
    if [[ "$startup_output" =~ STARTUP_FAILED:([0-9]+) ]]; then
        local startup_error_code="${BASH_REMATCH[1]}"
        progress_fail "Startup failed (exit code: $startup_error_code)"
        
        progress_phase "🧹" "Rollback" "├─"
        progress_task "Stopping containers"
        
        # Enhanced rollback with verification
        if docker_run "compose" "$config" "$project_dir" down; then
            progress_done
        else
            progress_fail "Some containers may still be running"
        fi
        
        # Clean up temp config file on startup failure
        if [[ -f "$TEMP_CONFIG_FILE" ]]; then
            rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
        fi
        
        progress_phase_done
        progress_complete "Startup failed"
        return $startup_error_code
    fi
    
    progress_phase_done "Services started"

    # Provisioning phase
    progress_phase "🔧" "Provisioning environment..." "└─"
    
    # Wait for container to be ready with enhanced error checking
    progress_task "Initializing container"
    local max_attempts=30
    local attempt=1
    local container_ready=false
    
    while [[ $attempt -le $max_attempts ]]; do
        # Check if container exists first
        if ! docker_cmd inspect "${container_name}" >/dev/null 2>&1; then
            progress_fail "Container '${container_name}' does not exist"
            progress_phase_done
            progress_complete "Container initialization failed"
            return 1
        fi
        
        # Use docker_cmd to handle sudo if needed, and check container is running
        local container_status
        if ! container_status=$(docker_cmd inspect "${container_name}" --format='{{.State.Status}}' 2>/dev/null); then
            progress_fail "Failed to check container status"
            progress_phase_done
            progress_complete "Container initialization failed"
            return 1
        fi
        
        if [[ "$container_status" != "running" ]]; then
            if [[ $attempt -eq $max_attempts ]]; then
                echo "❌ Container failed to start or is not running (status: $container_status)"
                echo "💡 Check container logs: vm logs"
                echo "💡 Try rebuilding: vm provision"
                echo "💡 Container may have exited due to configuration errors"
                
                # Show container exit code if available
                local exit_code
                if exit_code=$(docker_cmd inspect "${container_name}" --format='{{.State.ExitCode}}' 2>/dev/null); then
                    echo "💡 Container exit code: $exit_code"
                fi
                return 1
            fi
        else
            # Also verify we can exec into it
            if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
                progress_done
                container_ready=true
                break
            elif [[ $attempt -eq $max_attempts ]]; then
                echo "❌ Container is running but not responding to exec commands"
                echo "💡 Container may be starting up. Try again in a moment."
                echo "💡 Check container logs: vm logs"
                echo "💡 Container processes may not be fully initialized"
                return 1
            fi
        fi
        
        progress_update "."
        sleep 2
        ((attempt++))
    done
    
    if [[ "$container_ready" != "true" ]]; then
        progress_fail "Initialization timed out after $max_attempts attempts"
        progress_phase_done
        progress_complete "Container initialization failed"
        return 1
    fi

    # Copy config file to container with enhanced error handling and validation
    echo "📋 Loading project configuration..."
    
    # Validate temp config file exists and is readable
    if [[ ! -f "$TEMP_CONFIG_FILE" ]]; then
        echo "❌ Temporary configuration file not found: $TEMP_CONFIG_FILE"
        return 1
    fi
    
    if [[ ! -r "$TEMP_CONFIG_FILE" ]]; then
        echo "❌ Cannot read temporary configuration file: $TEMP_CONFIG_FILE"
        return 1
    fi
    
    # First attempt to copy configuration
    if ! docker_cmd cp "$TEMP_CONFIG_FILE" "$(printf '%q' "${container_name}"):/tmp/vm-config.json" 2>/dev/null; then
        echo "❌ Configuration loading failed on first attempt"
        echo "💡 Diagnosing container state..."
        
        # Enhanced container diagnostics
        local container_status
        if ! container_status=$(docker_cmd inspect "${container_name}" --format='{{.State.Status}}' 2>/dev/null); then
            echo "❌ Cannot inspect container - it may have been removed"
            return 1
        fi
        
        if [[ "$container_status" != "running" ]]; then
            echo "❌ Container has stopped unexpectedly (status: $container_status)"
            echo "💡 Check container logs: vm logs"
            
            # Show container exit details if available
            local exit_code
            if exit_code=$(docker_cmd inspect "${container_name}" --format='{{.State.ExitCode}}' 2>/dev/null); then
                echo "💡 Container exit code: $exit_code"
            fi
            return 1
        fi
        
        # Check if container filesystem is accessible
        if ! docker_cmd exec "${container_name}" test -w /tmp 2>/dev/null; then
            echo "❌ Container /tmp directory is not writable"
            echo "💡 Container may have filesystem issues or security restrictions"
            return 1
        fi
        
        # Retry the copy operation with detailed error reporting
        echo "🔄 Retrying configuration copy (container status: $container_status)..."
        sleep 2
        
        if ! docker_cmd cp "$TEMP_CONFIG_FILE" "$(printf '%q' "${container_name}"):/tmp/vm-config.json" 2>&1; then
            echo "❌ Configuration loading failed after retry"
            echo "💡 Possible causes:"
            echo "   - Container filesystem permissions"
            echo "   - Docker daemon issues"
            echo "   - Container security policies"
            echo "   - Insufficient disk space in container"
            return 1
        fi
    fi
    
    # Validate that the file was actually copied successfully
    if ! docker_cmd exec "${container_name}" test -f /tmp/vm-config.json 2>/dev/null; then
        echo "❌ Configuration file validation failed - file not found in container"
        return 1
    fi
    
    # Verify file is readable in container
    if ! docker_cmd exec "${container_name}" test -r /tmp/vm-config.json 2>/dev/null; then
        echo "❌ Configuration file is not readable in container"
        return 1
    fi
    
    progress_done

    # Fix volume permissions before Ansible
    progress_task "Setting up permissions"
    local project_user
    project_user=$(echo "$config" | yq -r '.vm.user // "developer"')
    if docker_run "exec" "$config" "$project_dir" chown -R "$(printf '%q' "$project_user"):$(printf '%q' "$project_user")" "/home/$(printf '%q' "$project_user")/.nvm" "/home/$(printf '%q' "$project_user")/.cache"; then
        progress_done
    else
        progress_done  # Non-critical, so we still mark as done
    fi

    # VM tool directory is already mounted read-only via docker-compose

    # Run Ansible provisioning
    progress_task "Running Ansible provisioning"

    # Check if debug mode is enabled
    ANSIBLE_VERBOSITY=""
    ANSIBLE_DIFF=""
    
    if [[ "${VM_DEBUG:-}" = "true" ]] || [[ "${DEBUG:-}" = "true" ]]; then
        progress_done
        echo "🐛 Debug mode enabled - showing detailed Ansible output"
        ANSIBLE_VERBOSITY="-vvv"
        ANSIBLE_DIFF="--diff"
        # In debug mode, show output directly
        if docker_run "exec" "$config" "$project_dir" bash -c "
            ansible-playbook \
                -i localhost, \
                -c local \
                $ANSIBLE_VERBOSITY \
                $ANSIBLE_DIFF \
                /vm-tool/shared/ansible/playbook.yml"; then
            echo "✅ Ansible provisioning completed successfully"
        else
            ANSIBLE_EXIT_CODE=$?
            echo "❌ Ansible provisioning failed (exit code: $ANSIBLE_EXIT_CODE)"
        fi
    else
        # Create log file path
        ANSIBLE_LOG="/tmp/ansible-create-$(date +%Y%m%d-%H%M%S).log"
        
        # In normal mode, show progress dots while running
        docker_run "exec" "$config" "$project_dir" bash -c "
            ansible-playbook \
                -i localhost, \
                -c local \
                /vm-tool/shared/ansible/playbook.yml > $ANSIBLE_LOG 2>&1" &
        
        # Get the PID of the background process
        local ansible_pid=$!
        
        # Show progress dots while Ansible is running
        while kill -0 $ansible_pid 2>/dev/null; do
            progress_update "."
            sleep 2
        done
        
        # Wait for the process to complete and get exit status
        wait $ansible_pid
        local ansible_exit=$?
        
        if [[ $ansible_exit -eq 0 ]]; then
            progress_done
            # Show summary of what was provisioned
            progress_subtask_done "System Configuration (hostname, locale, timezone)"
            progress_subtask_done "Development Tools (Node.js, npm, pnpm)"
            progress_subtask_done "Global packages installed"
            progress_subtask_done "Shell configuration complete"
        else
            progress_fail "Provisioning failed (exit code: $ansible_exit)"
            echo "📋 Full log saved in container at: $ANSIBLE_LOG"
            echo "💡 Tips:"
            echo "   - Run with VM_DEBUG=true vm create to see detailed error output"
            echo "   - View the log: vm exec cat $ANSIBLE_LOG"
            echo "   - Or copy it: docker cp $(printf '%q' "${container_name}"):$(printf '%q' "$ANSIBLE_LOG") ./ansible-error.log"
        fi
    fi

    # Ensure supervisor services are started
    progress_task "Starting services"
    docker_run "exec" "$config" "$project_dir" bash -c "supervisorctl reread && supervisorctl update" >/dev/null 2>&1 || true
    progress_done

    # Clean up generated docker-compose.yml since containers are now running
    local compose_file
    compose_file="${project_dir}/docker-compose.yml"
    if [[ -f "$compose_file" ]]; then
        rm "$compose_file"
    fi

    # Complete the phase
    progress_phase_done "Environment ready"
    
    # Calculate total time
    local end_time=$(date +%s)
    local total_time=$((end_time - ${build_start:-$end_time}))
    local time_str="${total_time}s"
    
    progress_complete "VM $container_name ready!" "$time_str"

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
        docker_run "exec" "$config" "" su - "$project_user" -c "cd $(printf '%q' "$target_dir") && source ~/.zshrc && $(printf '%q' "$2")"
    elif [[ $# -gt 0 ]]; then
        # Run with all arguments
        # Escape all arguments individually
        local escaped_args=""
        for arg in "$@"; do
            escaped_args="$escaped_args $(printf '%q' "$arg")"
        done
        docker_run "exec" "$config" "" su - "$project_user" -c "cd $(printf '%q' "$target_dir") && source ~/.zshrc && zsh$escaped_args"
    else
        # Interactive mode - use a simple approach that works
        local container_name
        container_name=$(get_project_container_name "$config")

        # Run an interactive shell with the working directory set
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG docker_ssh: Executing: docker exec -it ${container_name} su - $project_user -c \"VM_TARGET_DIR=$(printf '%q' \"$target_dir\") exec zsh\"" >&2
        fi

        # Use sudo for proper signal handling
        # This ensures proper Ctrl+C behavior (single tap interrupts, double tap to detach)
        docker_cmd exec -it "${container_name}" sudo -u "$project_user" sh -c "VM_TARGET_DIR=$(printf '%q' \"$target_dir\") exec zsh"

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

    # Check if container exists with enhanced diagnostics
    if ! docker_cmd inspect "${container_name}" >/dev/null 2>&1; then
        echo "❌ Container '${container_name}' doesn't exist"
        echo "💡 Use 'vm create' to set up the environment first"
        echo "💡 Or check if you're in the correct project directory"
        return 1
    fi
    
    # Get current container status for better error reporting
    local current_status
    if ! current_status=$(docker_cmd inspect "${container_name}" --format='{{.State.Status}}' 2>/dev/null); then
        echo "❌ Cannot determine container status"
        return 1
    fi
    
    # Check if container is already running
    if [[ "$current_status" == "running" ]]; then
        echo "✅ Container '${container_name}' is already running"
        # Skip to ready check
    else
        echo "🚀 Starting container '${container_name}' (current status: $current_status)..."
        
        # Start the container with enhanced error handling
        if ! docker_cmd start "${container_name}" "$@"; then
            local start_error_code=$?
            echo "❌ Failed to start container '${container_name}' (exit code: $start_error_code)"
            
            # Provide specific troubleshooting based on container state
            local exit_code
            if exit_code=$(docker_cmd inspect "${container_name}" --format='{{.State.ExitCode}}' 2>/dev/null); then
                echo "💡 Container exit code: $exit_code"
            fi
            
            echo "💡 Troubleshooting steps:"
            echo "   1. Check container logs: vm logs"
            echo "   2. Try recreating: vm destroy && vm create"
            echo "   3. Check Docker daemon status"
            echo "   4. Verify disk space and permissions"
            
            return $start_error_code
        fi
    fi

    # Wait for container to be ready with enhanced monitoring
    echo "⏳ Verifying container readiness..."
    local max_attempts=15
    local attempt=1
    local container_ready=false
    
    while [[ $attempt -le $max_attempts ]]; do
        # First verify container is still running
        local runtime_status
        if ! runtime_status=$(docker_cmd inspect "${container_name}" --format='{{.State.Status}}' 2>/dev/null); then
            echo "❌ Cannot check container status during startup"
            return 1
        fi
        
        if [[ "$runtime_status" != "running" ]]; then
            echo "❌ Container stopped during startup (status: $runtime_status)"
            
            # Show exit details
            local exit_code
            if exit_code=$(docker_cmd inspect "${container_name}" --format='{{.State.ExitCode}}' 2>/dev/null); then
                echo "💡 Container exit code: $exit_code"
            fi
            
            echo "💡 Check container logs: vm logs"
            return 1
        fi
        
        # Test if container is responsive
        if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
            echo "✅ Environment ready!"
            container_ready=true
            break
        fi
        
        if [[ $attempt -eq $max_attempts ]]; then
            echo "❌ Environment startup failed - container not responding"
            echo "💡 Container is running but not accepting exec commands"
            echo "💡 This may indicate:"
            echo "   - Container processes still starting"
            echo "   - Security policies blocking exec"
            echo "   - Container in unhealthy state"
            echo "💡 Try: vm logs to see container output"
            return 1
        fi
        
        echo "⏳ Waiting for container readiness... ($attempt/$max_attempts)"
        sleep 1
        ((attempt++))
    done
    
    if [[ "$container_ready" != "true" ]]; then
        echo "❌ Container startup verification failed"
        return 1
    fi

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

    # Stop the container directly (not using docker-compose) with error handling
    local container_name
    container_name=$(get_project_container_name "$config")
    
    # Check if container exists and is running
    if ! docker_cmd inspect "${container_name}" >/dev/null 2>&1; then
        echo "⚠️  Container '${container_name}' does not exist"
        return 0
    fi
    
    if ! docker_cmd inspect "${container_name}" --format='{{.State.Status}}' 2>/dev/null | grep -q "running"; then
        echo "⚠️  Container '${container_name}' is already stopped"
        return 0
    fi
    
    if ! docker_cmd stop "${container_name}" "$@"; then
        echo "❌ Failed to stop container gracefully"
        echo "💡 Trying force stop..."
        if ! docker_cmd kill "${container_name}" 2>/dev/null; then
            echo "❌ Failed to force stop container"
            return 1
        fi
        echo "⚠️  Container force stopped"
    fi
}

docker_destroy() {
    local config="$1"
    local project_dir="$2"
    shift 2

    # Get container name for user feedback
    local container_name
    container_name=$(get_project_container_name "$config")

    # Check if progress reporter is already initialized
    if [[ -z "$PROGRESS_IN_PROGRESS" ]] && [[ -z "$PROGRESS_LAST_LINE" ]]; then
        # Initialize progress reporter only if not already initialized
        progress_init "VM Operation" "$container_name"
    fi
    
    # DESTROY PHASE
    progress_phase "🗑️" "DESTROY PHASE"

    # Create a secure temporary file for the config
    TEMP_CONFIG_FILE=$(create_temp_file "vm-config.XXXXXX")

    # Generate docker-compose.yml temporarily for destroy operation
    progress_task "Preparing cleanup"
    echo "$config" > "$TEMP_CONFIG_FILE"
    "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"
    progress_done

    # Run docker compose down with volumes and parse output
    progress_task "Cleaning up resources"
    
    # Run docker compose down and capture output for progress
    local destroy_output=""
    while IFS= read -r line; do
        destroy_output+="$line"$'\n'
        case "$line" in
            *"Container"*"Removed"*)
                progress_subtask_done "Container removed"
                ;;
            *"Volume"*"Removed"*)
                # Count volumes being removed
                if [[ "$line" =~ "Volume.*Removed" ]]; then
                    volume_count=$(echo "$line" | grep -o "Removed" | wc -l)
                    progress_subtask_done "Volumes removed ($volume_count)"
                fi
                ;;
            *"Network"*"Removed"*)
                progress_subtask_done "Network removed"
                ;;
        esac
    done < <(docker_run "down" "$config" "$project_dir" -v "$@" 2>&1)

    # Clean up the generated docker-compose.yml after destroy
    local compose_file
    compose_file="${project_dir}/docker-compose.yml"
    if [[ -f "$compose_file" ]]; then
        rm "$compose_file"
    fi
    
    progress_phase_done "Destruction complete"
    progress_complete "VM $container_name destroyed"
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
    TEMP_CONFIG_FILE=$(create_temp_file "vm-config.XXXXXX")
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

    # Get container name for display
    local container_name
    container_name=$(get_project_container_name "$config")
    
    # Initialize progress reporter
    progress_init "VM Provision" "$container_name"
    
    # REBUILD PHASE
    progress_phase "🔄" "REBUILD PHASE"

    # Create a secure temporary file for the config
    TEMP_CONFIG_FILE=$(create_temp_file "vm-config.XXXXXX")

    # Generate fresh docker-compose.yml for provisioning
    progress_task "Preparing configuration"
    echo "$config" > "$TEMP_CONFIG_FILE"
    "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"
    progress_done

    # Build container phase
    progress_phase "🔨" "Rebuilding container..." "├─"
    
    # Track build start time
    local build_start=$(date +%s)
    
    # Run docker compose build with progress tracking
    local build_output=""
    local build_success=true
    
    # Capture build output while showing progress
    while IFS= read -r line; do
        build_output+="$line"$'\n'
        case "$line" in
            *"[internal]"*)
                progress_subtask_done "Loading build definitions"
                ;;
            *"FROM"*"ubuntu"*)
                progress_subtask_done "Using Ubuntu 24.04 base"
                ;;
            "#"*"["*"/"*"]"*)
                # Extract step info
                if [[ "$line" =~ \[([0-9]+)/([0-9]+)\] ]]; then
                    current="${BASH_REMATCH[1]}"
                    total="${BASH_REMATCH[2]}"
                    progress_subtask_done "Building layers ($current/$total)"
                fi
                ;;
            *"naming to"*)
                progress_subtask_done "Image rebuilt"
                ;;
        esac
    done < <(docker_run "compose" "$config" "$project_dir" build 2>&1 || echo "BUILD_FAILED:$?")
    
    # Check if build failed
    if [[ "$build_output" =~ BUILD_FAILED:([0-9]+) ]]; then
        local build_error_code="${BASH_REMATCH[1]}"
        progress_fail "Build failed (exit code: $build_error_code)"
        
        progress_phase "🧹" "Cleanup" "├─"
        progress_task "Removing build artifacts"
        
        if docker_run "compose" "$config" "$project_dir" down --remove-orphans; then
            progress_done
        else
            progress_fail "Some artifacts may remain"
        fi
        
        # Clean up temp config file on provision failure
        if [[ -f "$TEMP_CONFIG_FILE" ]]; then
            rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
        fi
        
        progress_phase_done
        progress_complete "Provision failed"
        return $build_error_code
    fi
    
    progress_phase_done "Build complete"
    
    # Start services phase
    progress_phase "📦" "Restarting services..." "├─"
    
    # Run docker compose up with progress tracking
    local startup_output=""
    while IFS= read -r line; do
        startup_output+="$line"$'\n'
        case "$line" in
            *"Container"*"Started"*)
                progress_subtask_done "Container restarted"
                ;;
        esac
    done < <(docker_run "compose" "$config" "$project_dir" up -d "$@" 2>&1 || echo "STARTUP_FAILED:$?")
    
    # Check if startup failed
    if [[ "$startup_output" =~ STARTUP_FAILED:([0-9]+) ]]; then
        local startup_error_code="${BASH_REMATCH[1]}"
        progress_fail "Startup failed (exit code: $startup_error_code)"
        
        progress_phase "🧹" "Rollback" "├─"
        progress_task "Stopping containers"
        
        if docker_run "compose" "$config" "$project_dir" down; then
            progress_done
        else
            progress_fail "Some containers may still be running"
        fi
        
        # Clean up temp config file on provision failure
        if [[ -f "$TEMP_CONFIG_FILE" ]]; then
            rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
        fi
        
        progress_phase_done
        progress_complete "Provision failed"
        return $startup_error_code
    fi
    
    progress_phase_done "Services restarted"

    # Provisioning phase
    progress_phase "🔧" "Re-provisioning environment..." "└─"
    
    # Wait for container to be ready
    progress_task "Waiting for container readiness"
    local max_attempts=30
    local attempt=1
    local container_ready=false
    
    while [[ $attempt -le $max_attempts ]]; do
        if [[ $attempt -gt 1 ]]; then
            sleep 1
        fi
        
        # Check if container is still running
        if ! docker_cmd ps --format "table {{.Names}}" | grep -q "^${container_name}$"; then
            progress_fail "Container stopped unexpectedly"
            if [[ -f "$TEMP_CONFIG_FILE" ]]; then
                rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
            fi
            progress_phase_done
            progress_complete "Provision failed"
            return 1
        fi
        
        # Test if container is responsive
        if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
            progress_done
            container_ready=true
            break
        fi
        
        progress_update "."
        
        if [[ $attempt -eq $max_attempts ]]; then
            progress_fail "Container not responding after $max_attempts attempts"
            if [[ -f "$TEMP_CONFIG_FILE" ]]; then  
                rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
            fi
            progress_phase_done
            progress_complete "Provision failed"
            return 1
        fi
        
        ((attempt++))
    done
    
    if [[ "$container_ready" != "true" ]]; then
        progress_fail "Container not ready"
        if [[ -f "$TEMP_CONFIG_FILE" ]]; then
            rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
        fi
        progress_phase_done
        progress_complete "Provision failed"
        return 1
    fi

    progress_task "Loading project configuration"
    
    # Validate temp config file exists and is readable
    if [[ ! -f "$TEMP_CONFIG_FILE" ]]; then
        progress_fail "Temporary configuration file not found"
        progress_phase_done
        progress_complete "Configuration loading failed"
        return 1
    fi
    
    if [[ ! -r "$TEMP_CONFIG_FILE" ]]; then
        progress_fail "Cannot read temporary configuration file"
        progress_phase_done
        progress_complete "Configuration loading failed"
        return 1
    fi
    
    # Copy configuration to container
    if ! docker_cmd cp "$TEMP_CONFIG_FILE" "$(printf '%q' "${container_name}"):/tmp/vm-config.json" 2>/dev/null; then
        sleep 2
        if ! docker_cmd cp "$TEMP_CONFIG_FILE" "$(printf '%q' "${container_name}"):/tmp/vm-config.json" 2>&1; then
            progress_fail "Configuration loading failed"
            if [[ -f "$TEMP_CONFIG_FILE" ]]; then
                rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
            fi
            progress_phase_done
            progress_complete "Configuration loading failed"
            return 1
        fi
    fi
    
    # Validate that the file was actually copied successfully
    if ! docker_cmd exec "${container_name}" test -f /tmp/vm-config.json 2>/dev/null; then
        progress_fail "Configuration file not found in container"
        if [[ -f "$TEMP_CONFIG_FILE" ]]; then
            rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
        fi
        progress_phase_done
        progress_complete "Configuration validation failed"
        return 1
    fi
    
    progress_done

    # Fix volume permissions before Ansible
    progress_task "Setting up permissions"
    local project_user
    project_user=$(echo "$config" | yq -r '.vm.user // "developer"')
    if docker_run "exec" "$config" "$project_dir" chown -R "$(printf '%q' "$project_user"):$(printf '%q' "$project_user")" "/home/$(printf '%q' "$project_user")/.nvm" "/home/$(printf '%q' "$project_user")/.cache"; then
        progress_done
    else
        progress_done  # Non-critical, so we still mark as done
    fi

    # Run Ansible provisioning
    progress_task "Running Ansible provisioning"

    # Check if debug mode is enabled
    ANSIBLE_VERBOSITY=""
    ANSIBLE_DIFF=""
    
    if [[ "${VM_DEBUG:-}" = "true" ]] || [[ "${DEBUG:-}" = "true" ]]; then
        progress_done
        echo "🐛 Debug mode enabled - showing detailed Ansible output"
        ANSIBLE_VERBOSITY="-vvv"
        ANSIBLE_DIFF="--diff"
        # In debug mode, show output directly
        if docker_run "exec" "$config" "$project_dir" bash -c "
            ansible-playbook \
                -i localhost, \
                -c local \
                $ANSIBLE_VERBOSITY \
                $ANSIBLE_DIFF \
                /vm-tool/shared/ansible/playbook.yml"; then
            echo "✅ Ansible provisioning completed successfully"
        else
            ANSIBLE_EXIT_CODE=$?
            echo "❌ Ansible provisioning failed (exit code: $ANSIBLE_EXIT_CODE)"
        fi
    else
        # Create log file path
        ANSIBLE_LOG="/tmp/ansible-provision-$(date +%Y%m%d-%H%M%S).log"
        
        # In normal mode, show progress dots while running
        docker_run "exec" "$config" "$project_dir" bash -c "
            ansible-playbook \
                -i localhost, \
                -c local \
                /vm-tool/shared/ansible/playbook.yml > $ANSIBLE_LOG 2>&1" &
        
        # Get the PID of the background process
        local ansible_pid=$!
        
        # Show progress dots while Ansible is running
        while kill -0 $ansible_pid 2>/dev/null; do
            progress_update "."
            sleep 2
        done
        
        # Wait for the process to complete and get exit status
        wait $ansible_pid
        local ansible_exit=$?
        
        if [[ $ansible_exit -eq 0 ]]; then
            progress_done
            # Show summary of what was provisioned
            progress_subtask_done "System Configuration (hostname, locale, timezone)"
            progress_subtask_done "Development Tools (Node.js, npm, pnpm)"
            progress_subtask_done "Global packages installed"
            progress_subtask_done "Shell configuration complete"
        else
            progress_fail "Provisioning failed (exit code: $ansible_exit)"
            echo "📋 Full log saved in container at: $ANSIBLE_LOG"
            echo "💡 Tips:"
            echo "   - Run with VM_DEBUG=true vm provision to see detailed error output"
            echo "   - View the log: vm exec cat $ANSIBLE_LOG"
            echo "   - Or copy it: docker cp $(printf '%q' "${container_name}"):$(printf '%q' "$ANSIBLE_LOG") ./ansible-error.log"
        fi
    fi

    # Ensure supervisor services are started
    progress_task "Starting services"
    docker_run "exec" "$config" "$project_dir" bash -c "supervisorctl reread && supervisorctl update" >/dev/null 2>&1 || true
    progress_done

    # Clean up generated docker-compose.yml since containers are now running
    local compose_file
    compose_file="${project_dir}/docker-compose.yml"
    if [[ -f "$compose_file" ]]; then
        rm "$compose_file"
    fi
    
    # Clean up temp config file after successful provisioning
    if [[ -f "$TEMP_CONFIG_FILE" ]]; then
        rm -f "$TEMP_CONFIG_FILE" 2>/dev/null || true
    fi

    # Complete the phase
    progress_phase_done "Environment ready"
    
    # Calculate total time
    local end_time=$(date +%s)
    local total_time=$((end_time - ${build_start:-$end_time}))
    local time_str="${total_time}s"
    
    progress_complete "VM $container_name re-provisioned!" "$time_str"
    
    echo "💡 Use 'vm ssh' to connect to the environment"
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

# Handle preset commands - Phase B implementation point
handle_preset_command() {
    local subcommand="${1:-}"
    shift
    
    case "$subcommand" in
        "list")
            preset_list_command
            ;;
        "show")
            preset_show_command "$@"
            ;;
        "")
            echo "❌ Missing preset subcommand" >&2
            echo ""
            echo "Usage: vm preset <subcommand>"
            echo ""
            echo "Available subcommands:"
            echo "  list              List all available presets"
            echo "  show <name>       Show detailed configuration for a preset"
            echo ""
            echo "Examples:"
            echo "  vm preset list"
            echo "  vm preset show react"
            echo "  vm preset show django.yaml"
            return 1
            ;;
        *)
            echo "❌ Unknown preset subcommand: $subcommand" >&2
            echo ""
            echo "Available subcommands:"
            echo "  list              List all available presets"
            echo "  show <name>       Show detailed configuration for a preset"
            echo ""
            echo "Use 'vm preset list' to see available presets"
            return 1
            ;;
    esac
}

# List all available presets with descriptions
preset_list_command() {
    local presets_dir="$SCRIPT_DIR/configs/presets"
    
    # Check if presets directory exists
    if [[ ! -d "$presets_dir" ]]; then
        echo "❌ Presets directory not found: $presets_dir" >&2
        return 1
    fi
    
    # Check if directory is empty
    local preset_files=($(find "$presets_dir" -name "*.yaml" -type f 2>/dev/null))
    if [[ ${#preset_files[@]} -eq 0 ]]; then
        echo "ℹ️  No presets found in $presets_dir"
        return 0
    fi
    
    echo "Available Configuration Presets:"
    echo "================================="
    echo ""
    
    # Table header
    printf "%-15s %-12s %s\n" "Name" "Type" "Description"
    printf "%-15s %-12s %s\n" "----" "----" "-----------"
    
    # Process each preset file
    for preset_file in "${preset_files[@]}"; do
        local preset_name=$(basename "$preset_file" .yaml)
        local preset_type="General"
        local description="No description available"
        
        # Extract metadata from the preset file
        if [[ -r "$preset_file" ]]; then
            # Try to extract the preset name and description from YAML
            local yaml_name=$(grep -E "^\s*name:\s*[\"']?(.+)[\"']?\s*$" "$preset_file" 2>/dev/null | sed -E 's/^\s*name:\s*[\"'"'"']?(.+)[\"'"'"']?\s*$/\1/' | head -1)
            local yaml_desc=$(grep -E "^\s*description:\s*[\"']?(.+)[\"']?\s*$" "$preset_file" 2>/dev/null | sed -E 's/^\s*description:\s*[\"'"'"']?(.+)[\"'"'"']?\s*$/\1/' | head -1)
            
            # Use extracted values if available
            [[ -n "$yaml_desc" ]] && description="$yaml_desc"
            
            # Determine preset type based on content and name
            if grep -q "django\|flask\|fastapi" "$preset_file" 2>/dev/null; then
                preset_type="Python Web"
            elif grep -q "react\|vue\|angular\|next" "$preset_file" 2>/dev/null; then
                preset_type="Frontend"
            elif grep -q "nodejs\|express\|npm" "$preset_file" 2>/dev/null; then
                preset_type="Node.js"
            elif grep -q "rails\|ruby" "$preset_file" 2>/dev/null; then
                preset_type="Ruby"
            elif grep -q "docker\|kubernetes" "$preset_file" 2>/dev/null; then
                preset_type="DevOps"
            elif grep -q "python" "$preset_file" 2>/dev/null; then
                preset_type="Python"
            fi
        fi
        
        # Truncate description if too long
        if [[ ${#description} -gt 50 ]]; then
            description="${description:0:47}..."
        fi
        
        printf "%-15s %-12s %s\n" "$preset_name" "$preset_type" "$description"
    done
    
    echo ""
    echo "Use 'vm preset show <name>' to view detailed configuration for any preset."
}

# Show detailed configuration for a specific preset
preset_show_command() {
    local preset_name="${1:-}"
    
    if [[ -z "$preset_name" ]]; then
        echo "❌ Missing preset name" >&2
        echo ""
        echo "Usage: vm preset show <name>"
        echo ""
        echo "Examples:"
        echo "  vm preset show react"
        echo "  vm preset show django.yaml"
        echo ""
        echo "Use 'vm preset list' to see available presets."
        return 1
    fi
    
    local presets_dir="$SCRIPT_DIR/configs/presets"
    
    # Handle preset name with or without .yaml extension
    local preset_file
    if [[ "$preset_name" == *.yaml ]]; then
        preset_file="$presets_dir/$preset_name"
    else
        preset_file="$presets_dir/$preset_name.yaml"
    fi
    
    # Check if preset file exists
    if [[ ! -f "$preset_file" ]]; then
        echo "❌ Preset not found: $preset_name" >&2
        echo ""
        
        # Suggest similar presets
        local available_presets=($(find "$presets_dir" -name "*.yaml" -type f 2>/dev/null | xargs -I {} basename {} .yaml))
        if [[ ${#available_presets[@]} -gt 0 ]]; then
            echo "Available presets:"
            for available in "${available_presets[@]}"; do
                echo "  - $available"
            done
            echo ""
            echo "Use 'vm preset list' for detailed information."
        else
            echo "No presets available in $presets_dir"
        fi
        return 1
    fi
    
    # Check file permissions
    if [[ ! -r "$preset_file" ]]; then
        echo "❌ Cannot read preset file: $preset_file" >&2
        echo "Check file permissions." >&2
        return 1
    fi
    
    echo "Preset Configuration: $(basename "$preset_file" .yaml)"
    echo "======================================================"
    echo ""
    
    # Display file with helpful annotations
    local in_section=""
    local line_number=0
    
    while IFS= read -r line; do
        ((line_number++))
        
        # Skip YAML document separator and comments at the start
        if [[ "$line" =~ ^---$ ]] || [[ "$line" =~ ^#.*$ && $line_number -le 5 ]]; then
            continue
        fi
        
        # Detect sections and add explanatory comments
        if [[ "$line" =~ ^[a-zA-Z_][a-zA-Z0-9_]*: ]]; then
            local section=$(echo "$line" | cut -d: -f1)
            
            case "$section" in
                "preset")
                    if [[ "$in_section" != "preset" ]]; then
                        echo "# Preset metadata and information"
                        in_section="preset"
                    fi
                    ;;
                "npm_packages")
                    if [[ "$in_section" != "npm_packages" ]]; then
                        echo ""
                        echo "# Node.js packages to install globally"
                        in_section="npm_packages"
                    fi
                    ;;
                "pip_packages")
                    if [[ "$in_section" != "pip_packages" ]]; then
                        echo ""
                        echo "# Python packages to install"
                        in_section="pip_packages"
                    fi
                    ;;
                "cargo_packages")
                    if [[ "$in_section" != "cargo_packages" ]]; then
                        echo ""
                        echo "# Rust packages to install"
                        in_section="cargo_packages"
                    fi
                    ;;
                "ports")
                    if [[ "$in_section" != "ports" ]]; then
                        echo ""
                        echo "# Network ports to expose from the VM"
                        in_section="ports"
                    fi
                    ;;
                "services")
                    if [[ "$in_section" != "services" ]]; then
                        echo ""
                        echo "# System services configuration (databases, caches, etc.)"
                        in_section="services"
                    fi
                    ;;
                "environment")
                    if [[ "$in_section" != "environment" ]]; then
                        echo ""
                        echo "# Environment variables to set"
                        in_section="environment"
                    fi
                    ;;
                "aliases")
                    if [[ "$in_section" != "aliases" ]]; then
                        echo ""
                        echo "# Shell aliases to create"
                        in_section="aliases"
                    fi
                    ;;
            esac
        fi
        
        echo "$line"
    done < "$preset_file"
    
    echo ""
    echo "To use this preset:"
    echo "  vm --preset $(basename "$preset_file" .yaml) create"
    echo "  vm --preset $(basename "$preset_file" .yaml) --config path/to/vm.yaml create"
}

# Parse command line arguments manually for better control
CUSTOM_CONFIG=""
# DEBUG_MODE is deprecated, using VM_DEBUG instead
DRY_RUN="false"
AUTO_LOGIN="true"
USE_PRESETS="true"
VM_INTERACTIVE="false"
FORCED_PRESET=""
ARGS=()

# Manual argument parsing - much simpler and more reliable than getopt
while [[ $# -gt 0 ]]; do
    case "$1" in
        -c|--config)
            shift
            # Check if next argument exists and is not a flag or command
            if [[ $# -eq 0 ]] || [[ "$1" =~ ^- ]] || [[ "$1" =~ ^(init|generate|validate|preset|migrate|list|temp|create|start|stop|restart|ssh|destroy|status|provision|logs|exec|kill|help)$ ]]; then
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
        --no-preset)
            USE_PRESETS="false"
            shift
            ;;
        --interactive)
            VM_INTERACTIVE="true"
            shift
            ;;
        --preset)
            shift
            if [[ $# -eq 0 ]] || [[ "$1" =~ ^- ]]; then
                echo "❌ --preset requires a preset name (nodejs, python, etc.)" >&2
                exit 1
            fi
            FORCED_PRESET="$1"
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

# Skip main execution if only sourcing functions
if [[ "${SKIP_MAIN_EXECUTION:-}" == "true" ]]; then
    return 0 2>/dev/null || exit 0
fi

# Handle special commands
case "${1:-}" in
    "init")
        echo "✨ Creating new project configuration..."
        # Use shared config processor for init
        if [[ -n "$CUSTOM_CONFIG" ]] && [[ "$CUSTOM_CONFIG" != "__SCAN__" ]]; then
            init_config_file "$CUSTOM_CONFIG"
        else
            init_config_file
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
        # Validate configuration using the shared config processor
        if [[ -n "$CUSTOM_CONFIG" ]]; then
            validate_config_file "$CUSTOM_CONFIG"
        else
            validate_config_file
        fi
        ;;
    "preset")
        # Handle preset commands - delegate to preset handler
        shift
        handle_preset_command "$@"
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
        # Handle temp VM commands - delegate to vm-temporary.sh module
        shift
        source "$SCRIPT_DIR/vm-temporary.sh"
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

            # Initialize progress reporter for destroy operation
            progress_init "VM Operation" "$container_name"
            
            # Show confirmation in destroy phase
            echo -e "├─ ⚠️  Confirm destruction of $container_name? (y/N): \c"
            read -r response
            case "$response" in
                [yY]|[yY][eE][sS])
                    if [[ "$PROVIDER" = "docker" ]]; then
                        # The docker_destroy function will handle the rest of the progress reporting
                        docker_destroy "$CONFIG" "$PROJECT_DIR"
                    else
                        progress_phase "🗑️" "DESTROY PHASE"
                        progress_task "Destroying Vagrant VM"
                        VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant destroy -f
                        progress_done
                        progress_phase_done "Destruction complete"
                        progress_complete "Vagrant VM destroyed"
                    fi
                    ;;
                *)
                    echo "└─ ❌ Destruction cancelled"
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
            echo "DEBUG main: USE_PRESETS='$USE_PRESETS', FORCED_PRESET='$FORCED_PRESET'" >&2
        fi
        
        # Export preset configuration for config-processor.sh
        export VM_USE_PRESETS="$USE_PRESETS"
        if [[ -n "$FORCED_PRESET" ]]; then
            export VM_FORCED_PRESET="$FORCED_PRESET"
        fi
        
        # Use enhanced config loading with preset support
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
                        VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "cd $(printf '%q' \"$TARGET_DIR\") && exec /bin/zsh"
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
                    # Escape all arguments individually for safe passing to vagrant ssh -c
                    local escaped_command=""
                    for arg in "$@"; do
                        escaped_command="$escaped_command $(printf '%q' "$arg")"
                    done
                    VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "$escaped_command"
                    ;;
                "logs")
                    # Show service logs in Vagrant VM
                    echo "Showing service logs - Press Ctrl+C to stop..."
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
