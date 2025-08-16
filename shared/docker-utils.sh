#!/bin/bash
# Docker utility functions shared across VM tool scripts
# These functions handle Docker command execution with automatic sudo detection
# and provide compatibility between docker-compose and docker compose commands

# Docker wrapper to handle sudo requirements
# This function automatically detects if Docker requires sudo and executes accordingly
# Usage: docker_cmd [docker arguments...]
# Example: docker_cmd ps -a
docker_cmd() {
    if ! docker version >/dev/null; then
        sudo docker "$@"
    else
        docker "$@"
    fi
}

# Docker compose wrapper to handle both docker-compose and docker compose
# This function provides compatibility between the old docker-compose command
# and the new docker compose subcommand, while also handling sudo requirements
# Usage: docker_compose [compose arguments...]
# Example: docker_compose up -d
docker_compose() {
    # Check if we need sudo for docker
    local docker_prefix
    docker_prefix=""
    if ! docker version >/dev/null; then
        docker_prefix="sudo"
    fi

    if command -v docker-compose &> /dev/null; then
        if [[ -n "$docker_prefix" ]]; then
            $docker_prefix docker-compose "$@"
        else
            docker-compose "$@"
        fi
    else
        if [[ -n "$docker_prefix" ]]; then
            $docker_prefix docker compose "$@"
        else
            docker compose "$@"
        fi
    fi
}

# Construct Docker-specific mount argument for a validated directory and permissions
construct_mount_argument() {
    local source_dir="$1"
    local permission_flags="$2"

    # SECURITY: Re-validate the path immediately before use to prevent TOCTOU attacks
    # The symlink target could have changed between initial validation and mount construction
    local real_source
    if ! real_source=$(realpath "$source_dir" 2>/dev/null); then
        echo "❌ Error: Cannot resolve path '$source_dir'" >&2
        return 1
    fi

    # Re-run security validation on the resolved path to prevent TOCTOU
    # This ensures the symlink hasn't been changed to point to a dangerous location
    if ! validate_mount_security_atomic "$real_source"; then
        echo "❌ Error: Mount security re-validation failed for '$source_dir'" >&2
        echo "💡 The target may have changed since initial validation (TOCTOU protection)" >&2
        return 1
    fi

    # Build the mount argument with proper quoting to prevent command injection
    echo "-v $(printf '%q' "$real_source"):/workspace/$(basename "$source_dir")${permission_flags}"
}

# Check if Docker is accessible without sudo
# This function is used by test scripts to determine if Docker operations can proceed
# Returns 0 if Docker is accessible, 1 if it requires sudo or is not available
check_docker_access() {
    # VM operations require docker without sudo
    if docker version &>/dev/null 2>&1; then
        return 0
    fi
    
    # Docker is not accessible without sudo
    return 1
}