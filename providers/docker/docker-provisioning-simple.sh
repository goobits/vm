#!/bin/bash
# Simplified Docker provisioning script - Shell version
# Purpose: Generate docker-compose.yml from VM configuration using jq
# Usage: ./docker-provisioning-simple.sh <config-path> [project-dir]

set -e

# Function to generate docker-compose.yml
generate_docker_compose() {
    local config_path="$1"
    local project_dir="${2:-$(pwd)}"
    
    # Load and validate config
    if [[ ! -f "$config_path" ]]; then
        echo "Error: Config file not found: $config_path" >&2
        return 1
    fi
    
    local config
    if ! config="$(jq . "$config_path" 2>/dev/null)"; then
        echo "Error: Invalid JSON in config file: $config_path" >&2
        return 1
    fi
    
    # Get host user/group IDs for proper file permissions
    local host_uid="$(id -u)"
    local host_gid="$(id -g)"
    
    # Extract basic project data using jq
    local project_name="$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')"
    local project_hostname="$(echo "$config" | jq -r '.project.hostname')"
    local workspace_path="$(echo "$config" | jq -r '.project.workspace_path // "/workspace"')"
    local project_user="$(echo "$config" | jq -r '.vm.user // "vagrant"')"
    local timezone="$(echo "$config" | jq -r '.vm.timezone // "UTC"')"
    
    # Get VM tool path (use absolute path to avoid relative path issues)
    # The VM tool is always in the workspace directory where vm.sh is located
    local vm_tool_base_path="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
    
    # Use the vm-tool path directly from the host mount
    # Mount the vm-tool directory directly instead of copying
    local vm_tool_path="/vm-tool"
    
    # Generate ports section
    local ports_section=""
    local ports_count="$(echo "$config" | jq '.ports // {} | length')"
    if [[ "$ports_count" -gt 0 ]]; then
        local host_ip="$(echo "$config" | jq -r '.vm.port_binding // "127.0.0.1"')"
        ports_section="$(echo "$config" | jq -r --arg hostip "$host_ip" '
            .ports // {} | 
            to_entries | 
            map("      - \"" + $hostip + ":" + (.value | tostring) + ":" + (.value | tostring) + "\"") | 
            if length > 0 then "\n    ports:\n" + join("\n") else "" end
        ')"
    fi
    
    # Generate Claude sync volume
    local claude_sync_volume=""
    local claude_sync="$(echo "$config" | jq -r '.claude_sync // false')"
    if [[ "$claude_sync" == "true" ]]; then
        local host_path="$HOME/.claude/vms/$project_name"
        local container_path="/home/$project_user/.claude"
        claude_sync_volume="\\n      - $host_path:$container_path:delegated"
    fi
    
    # Generate Gemini sync volume
    local gemini_sync_volume=""
    local gemini_sync="$(echo "$config" | jq -r '.gemini_sync // false')"
    if [[ "$gemini_sync" == "true" ]]; then
        local host_path="$HOME/.gemini/vms/$project_name"
        local container_path="/home/$project_user/.gemini"
        gemini_sync_volume="\\n      - $host_path:$container_path:delegated"
    fi
    
    # Generate database persistence volumes
    local database_volumes=""
    local persist_databases="$(echo "$config" | jq -r '.persist_databases // false')"
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
    
    # Handle audio and GPU support
    local audio_env=""
    local audio_volumes=""
    local devices=()
    local groups=()
    
    if [[ "$(echo "$config" | jq -r '.services.audio.enabled // false')" == "true" ]]; then
        audio_env="\\n      - PULSE_SERVER=unix:/run/user/1000/pulse/native"
        audio_volumes="\\n      - \${XDG_RUNTIME_DIR}/pulse:/run/user/1000/pulse"
        devices+=("/dev/snd:/dev/snd")
        groups+=("audio")
    fi
    
    # Handle GPU support
    local gpu_env=""
    local gpu_volumes=""
    
    if [[ "$(echo "$config" | jq -r '.services.gpu.enabled // false')" == "true" ]]; then
        local gpu_type="$(echo "$config" | jq -r '.services.gpu.type // "auto"')"
        
        # NVIDIA GPU support
        if [[ "$gpu_type" == "nvidia" || "$gpu_type" == "auto" ]]; then
            gpu_env="\\n      - NVIDIA_VISIBLE_DEVICES=all\\n      - NVIDIA_DRIVER_CAPABILITIES=all"
        fi
        
        # DRI devices for Intel/AMD GPU access
        devices+=("/dev/dri:/dev/dri")
        groups+=("video" "render")
    fi
    
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
    local docker_compose_content="services:
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
      - TZ=$timezone$audio_env$gpu_env
    volumes:
      - $project_dir:$workspace_path:delegated
      - $vm_tool_base_path:$vm_tool_path:ro
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ${project_name}_nvm:/home/$project_user/.nvm
      - ${project_name}_cache:/home/$project_user/.cache
      - ${project_name}_config:/tmp$claude_sync_volume$gemini_sync_volume$database_volumes$audio_volumes$gpu_volumes$ports_section$devices_section$groups_section
    networks:
      - ${project_name}_network
    cap_add:
      - SYS_PTRACE
    security_opt:
      - seccomp:unconfined

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