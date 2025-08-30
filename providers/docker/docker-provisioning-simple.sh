#!/bin/bash
# Simplified Docker provisioning script - Shell version
# Purpose: Generate docker-compose.yml from VM configuration using jq
# Usage: ./docker-provisioning-simple.sh <config-path> [project-dir]

set -e
set -u

# Get VM tool directory for accessing default config and utilities
VM_TOOL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Source shared utilities
source "$VM_TOOL_DIR/shared/deep-merge.sh"
source "$VM_TOOL_DIR/shared/link-detector.sh"

# Generate docker-compose.yml from VM configuration
# Args: config_path (required), project_dir (optional, defaults to pwd)
# Creates docker-compose.yml with proper volumes, environment, and service configuration
generate_docker_compose() {
    local config_path="$1"
    local project_dir="${2:-$(pwd)}"

    # Load and merge config with defaults using standardized utility
    local default_config_path="$VM_TOOL_DIR/vm.yaml"
    local config

    if ! config="$(merge_project_config "$default_config_path" "$config_path")"; then
        echo "❌ Failed to merge project configuration with defaults" >&2
        return 1
    fi

    # Get host user/group IDs for proper file permissions
    local host_uid
    host_uid="$(id -u)"
    local host_gid
    host_gid="$(id -g)"

    # Extract basic project data using jq
    local project_name
    project_name="$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')"
    local project_hostname
    project_hostname="$(echo "$config" | jq -r '.project.hostname')"
    
    # Check if hostname is missing or null
    if [[ -z "$project_hostname" || "$project_hostname" == "null" ]]; then
        echo "❌ Error: Missing 'project.hostname' in vm.yaml" >&2
        echo "" >&2
        echo "Please add a hostname to your vm.yaml file:" >&2
        echo "" >&2
        echo "  project:" >&2
        echo "    hostname: dev.${project_name}.local" >&2
        echo "" >&2
        return 1
    fi
    local workspace_path
    workspace_path="$(echo "$config" | jq -r '.project.workspace_path // "/workspace"')"
    local project_user
    project_user="$(echo "$config" | jq -r '.vm.user // "developer"')"
    local timezone
    timezone="$(echo "$config" | jq -r '.vm.timezone // "UTC"')"

    # Get VM tool path (use absolute path to avoid relative path issues)
    # The VM tool is always in the workspace directory where vm.sh is located
    local vm_tool_base_path
    vm_tool_base_path="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

    # Use the vm-tool path directly from the host mount
    # Mount the vm-tool directory directly instead of copying
    local vm_tool_path="/vm-tool"

    # Generate ports section
    local ports_section=""
    local ports_count
    ports_count="$(echo "$config" | jq '.ports // {} | length')"
    if [[ "$ports_count" -gt 0 ]]; then
        local host_ip
        host_ip="$(echo "$config" | jq -r '.vm.port_binding // "127.0.0.1"')"
        ports_section="$(echo "$config" | jq -r --arg hostip "$host_ip" '
            .ports // {} |
            to_entries |
            map("      - \"" + $hostip + ":" + (.value | tostring) + ":" + (.value | tostring) + "\"") |
            if length > 0 then "\n    ports:\n" + join("\n") else "" end
        ')"
    fi

    # Collect explicit port mappings to avoid conflicts with port range
    local explicit_ports=""
    if [[ "$ports_count" -gt 0 ]]; then
        explicit_ports="$(echo "$config" | jq -r '.ports // {} | to_entries[] | .value' | tr '\n' ' ')"
    fi

    # Generate port range forwarding (skip explicit ports to avoid conflicts)
    local port_range
    port_range="$(echo "$config" | jq -r '.port_range // ""')"
    if [[ -n "$port_range" && "$port_range" =~ ^[0-9]+-[0-9]+$ ]]; then
        local range_start range_end
        range_start="$(echo "$port_range" | cut -d'-' -f1)"
        range_end="$(echo "$port_range" | cut -d'-' -f2)"
        local host_ip
        host_ip="$(echo "$config" | jq -r '.vm.port_binding // "127.0.0.1"')"
        
        if [[ $range_start -lt $range_end && $range_start -ge 1 && $range_end -le 65535 ]]; then
            local range_ports=""
            for port in $(seq "$range_start" "$range_end"); do
                # Skip if port already explicitly mapped in ports: section
                if [[ ! " $explicit_ports " =~ " $port " ]]; then
                    range_ports+="\n      - \"$host_ip:$port:$port\""
                fi
            done
            
            if [[ -n "$range_ports" ]]; then
                if [[ -n "$ports_section" ]]; then
                    # Append to existing ports section
                    ports_section="${ports_section}${range_ports}"
                else
                    # Create new ports section
                    ports_section="\n    ports:${range_ports}"
                fi
            fi
        fi
    fi

    # Generate Claude sync volume
    local claude_sync_volume=""
    local claude_sync
    claude_sync="$(echo "$config" | jq -r '.claude_sync // false')"
    if [[ "$claude_sync" == "true" ]]; then
        local host_path="$HOME/.claude/vms/$project_name"
        local container_path="/home/$project_user/.claude"
        claude_sync_volume="\\n      - $host_path:$container_path:delegated"
    fi

    # Generate Gemini sync volume
    local gemini_sync_volume=""
    local gemini_sync
    gemini_sync="$(echo "$config" | jq -r '.gemini_sync // false')"
    if [[ "$gemini_sync" == "true" ]]; then
        local host_path="$HOME/.gemini/vms/$project_name"
        local container_path="/home/$project_user/.gemini"
        gemini_sync_volume="\\n      - $host_path:$container_path:delegated"
    fi

    # Generate database persistence volumes
    local database_volumes=""
    local persist_databases
    persist_databases="$(echo "$config" | jq -r '.persist_databases // false')"
    if [[ "$persist_databases" == "true" ]]; then
        local vm_data_path="$project_dir/.vm/data"

        # Check each database service
        if [[ "$(echo "$config" | jq -r '.services.postgresql.enabled // false')" == "true" ]]; then
            database_volumes+="\\n      - $vm_data_path/postgres:/var/lib/postgresql:delegated"
        fi

        if [[ "$(echo "$config" | jq -r '.services.redis.enabled // false')" == "true" ]]; then
            database_volumes+="\\n      - $vm_data_path/redis:/var/lib/redis:delegated"
        fi

        if [[ "$(echo "$config" | jq -r '.services.mongodb.enabled // false')" == "true" ]]; then
            database_volumes+="\\n      - $vm_data_path/mongodb:/var/lib/mongodb:delegated"
        fi

        if [[ "$(echo "$config" | jq -r '.services.mysql.enabled // false')" == "true" ]]; then
            database_volumes+="\\n      - $vm_data_path/mysql:/var/lib/mysql:delegated"
        fi
    fi

    # Handle VM temp mounts (for vm temp command)
    local temp_mount_volumes=""
    if [[ -n "${VM_TEMP_MOUNTS:-}" ]]; then
        # VM_TEMP_MOUNTS contains space-separated "realpath:mountname:permission" pairs
        # Also supports legacy "realpath:mountname" format for backward compatibility
        for mount_mapping in $VM_TEMP_MOUNTS; do
            if [[ "$mount_mapping" == *:* ]]; then
                # Check if the last part is a valid permission (ro or rw)
                local last_part="${mount_mapping##*:}"
                if [[ "$last_part" == "ro" || "$last_part" == "rw" ]]; then
                    # New 3-part format: realpath:mountname:permission
                    # Remove the permission part to get realpath:mountname
                    local path_and_name="${mount_mapping%:*}"
                    local real_path="${path_and_name%:*}"
                    local mount_name="${path_and_name##*:}"
                    local permission="$last_part"
                    
                    # Apply permissions to Docker volume syntax
                    if [[ "$permission" == "ro" ]]; then
                        temp_mount_volumes+="\\n      - $real_path:$workspace_path/$mount_name:ro:delegated"
                    else
                        # Default to read-write for "rw"
                        temp_mount_volumes+="\\n      - $real_path:$workspace_path/$mount_name:delegated"
                    fi
                else
                    # Legacy 2-part format: realpath:mountname (default to rw)
                    local real_path="${mount_mapping%:*}"
                    local mount_name="${mount_mapping##*:}"
                    temp_mount_volumes+="\\n      - $real_path:$workspace_path/$mount_name:delegated"
                fi
            fi
        done
    fi

    # Handle audio and GPU support
    local audio_env=""
    local audio_volumes=""
    local devices=()
    local groups=()

    if [[ "$(echo "$config" | jq -r '.services.audio.enabled // true')" == "true" ]]; then
        # Use proper UID and runtime directory with fallbacks
        local runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$host_uid}"
        local pulse_socket="$runtime_dir/pulse/native"

        # Verify host PulseAudio socket exists before mounting
        if [[ -S "$pulse_socket" ]]; then
            audio_env="\\n      - PULSE_SERVER=unix:/run/user/$host_uid/pulse/native"
            audio_volumes="\\n      - $runtime_dir/pulse:/run/user/$host_uid/pulse"
            echo "📢 Audio: Mounting PulseAudio socket from $pulse_socket"
        else
            audio_env="\\n      - PULSE_RUNTIME_PATH=/run/user/$host_uid/pulse"
            echo "⚠️  Audio: Host PulseAudio socket not found at $pulse_socket, using system mode"
        fi

        devices+=("/dev/snd:/dev/snd")
        groups+=("audio")
    fi

    # Handle GPU support
    local gpu_env=""
    local gpu_volumes=""

    if [[ "$(echo "$config" | jq -r '.services.gpu.enabled // false')" == "true" ]]; then
        local gpu_type
        gpu_type="$(echo "$config" | jq -r '.services.gpu.type // "auto"')"

        # NVIDIA GPU support
        if [[ "$gpu_type" == "nvidia" || "$gpu_type" == "auto" ]]; then
            gpu_env="\\n      - NVIDIA_VISIBLE_DEVICES=all\\n      - NVIDIA_DRIVER_CAPABILITIES=all"
        fi

        # DRI devices for Intel/AMD GPU access
        devices+=("/dev/dri:/dev/dri")
        groups+=("video" "render")
    fi

    # Package linking volumes (npm, pip, cargo)
    local package_link_volumes=""
    
    # Check each package manager if enabled and packages are configured
    for pm in "npm" "pip" "cargo"; do
        local pm_enabled=""
        local packages_array=()
        
        # Check if this package manager is enabled
        case "$pm" in
            "npm")
                pm_enabled="$(echo "$config" | jq -r '.package_linking.npm // true')"
                local npm_packages
                npm_packages="$(echo "$config" | jq -r '.npm_packages[]? // empty')"
                if [[ -n "$npm_packages" ]]; then
                    while IFS= read -r package; do
                        [[ -z "$package" ]] && continue
                        packages_array+=("$package")
                    done <<< "$npm_packages"
                fi
                ;;
            "pip")
                pm_enabled="$(echo "$config" | jq -r '.package_linking.pip // false')"
                local pip_packages
                pip_packages="$(echo "$config" | jq -r '.pip_packages[]? // empty')"
                if [[ -n "$pip_packages" ]]; then
                    while IFS= read -r package; do
                        [[ -z "$package" ]] && continue
                        packages_array+=("$package")
                    done <<< "$pip_packages"
                fi
                ;;
            "cargo")
                pm_enabled="$(echo "$config" | jq -r '.package_linking.cargo // false')"
                local cargo_packages
                cargo_packages="$(echo "$config" | jq -r '.cargo_packages[]? // empty')"
                if [[ -n "$cargo_packages" ]]; then
                    while IFS= read -r package; do
                        [[ -z "$package" ]] && continue
                        packages_array+=("$package")
                    done <<< "$cargo_packages"
                fi
                ;;
        esac
        
        # Generate mounts if enabled and packages exist
        if [[ "$pm_enabled" == "true" ]] && [[ ${#packages_array[@]} -gt 0 ]] && command -v "$pm" >/dev/null 2>&1; then
            if declare -f generate_package_mounts >/dev/null 2>&1; then
                local linked_volumes
                linked_volumes=$(generate_package_mounts "$pm" "${packages_array[@]}")
                if [[ -n "$linked_volumes" ]]; then
                    while IFS= read -r volume; do
                        [[ -z "$volume" ]] && continue
                        package_link_volumes+="\\n      - $volume"
                    done <<< "$linked_volumes"
                fi
            fi
        fi
    done

    # Build consolidated devices and groups sections
    local devices_section=""
    if [[ ${#devices[@]} -gt 0 ]]; then
        devices_section="\\n    devices:"
        for device in "${devices[@]}"; do
            devices_section+="\\n      - $device"
        done
    fi

    local groups_section=""
    if [[ ${#groups[@]} -gt 0 ]]; then
        groups_section="\\n    group_add:"
        for group in "${groups[@]}"; do
            groups_section+="\\n      - $group"
        done
    fi

    # Create docker-compose.yml content
    local docker_compose_content
    docker_compose_content="services:
  $project_name:
    build:
      context: $vm_tool_base_path
      dockerfile: providers/docker/Dockerfile
      args:
        PROJECT_USER: \"$project_user\"
        PROJECT_UID: \"$host_uid\"
        PROJECT_GID: \"$host_gid\"
    container_name: $project_name-dev
    hostname: $project_hostname
    tty: true
    stdin_open: true
    environment:
      - LANG=en_US.UTF-8
      - LC_ALL=en_US.UTF-8
      - TZ=$timezone
      - PROJECT_USER=$project_user$audio_env$gpu_env
    volumes:$(if [[ "${VM_IS_TEMP:-}" != "true" ]]; then echo -e "\n      - $project_dir:$workspace_path:delegated"; fi)
      - $vm_tool_base_path:$vm_tool_path:ro
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ${project_name}_nvm:/home/$project_user/.nvm
      - ${project_name}_cache:/home/$project_user/.cache
      - ${project_name}_config:/tmp$claude_sync_volume$gemini_sync_volume$database_volumes$temp_mount_volumes$package_link_volumes$audio_volumes$gpu_volumes$devices_section$groups_section$ports_section
    networks:
      - ${project_name}_network
    # Security: Removed dangerous capabilities that create container escape risks
    # - SYS_PTRACE: Allows debugging/tracing processes, potential security risk
    # - seccomp:unconfined: Disables syscall filtering, removes critical security layer
    #
    # Minimal capabilities for development workflows:
    cap_add:
      - CHOWN        # Change file ownership (needed for development file operations)
      - SETUID       # Set user ID (needed for sudo and user switching)
      - SETGID       # Set group ID (needed for proper group permissions)
    # Note: Default seccomp profile remains enabled for security

networks:
  ${project_name}_network:
    driver: bridge

volumes:
  ${project_name}_nvm:
  ${project_name}_cache:
  ${project_name}_config:"

    # Write docker-compose.yml
    local output_path="$project_dir/docker-compose.yml"
    echo -e "$docker_compose_content" > "$output_path"
    echo "📄 Configuration generated at $output_path"
}

# Allow direct execution
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    config_path="$1"
    project_dir="${2:-$(pwd)}"

    if [[ -z "$config_path" ]]; then
        echo "Usage: $0 <config-path> [project-dir]" >&2
        exit 1
    fi

    generate_docker_compose "$config_path" "$project_dir"
fi