#!/bin/bash
# Docker utility functions shared across VM tool scripts
# These functions handle Docker command execution with automatic sudo detection
# and provide compatibility between docker-compose and docker compose commands

# Docker wrapper to handle sudo requirements
# This function automatically detects if Docker requires sudo and executes accordingly
# Usage: docker_cmd [docker arguments...]
# Example: docker_cmd ps -a
docker_cmd() {
    if ! docker version &>/dev/null 2>&1; then
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
    if ! docker version &>/dev/null 2>&1; then
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