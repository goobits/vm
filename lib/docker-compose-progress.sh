#!/bin/bash
# Docker Compose Progress Wrapper
# Parses Docker Compose output and reports progress using progress-reporter.sh

# Guard against multiple sourcing
if [[ -n "${DOCKER_COMPOSE_PROGRESS_LOADED:-}" ]]; then
    return 0
fi
DOCKER_COMPOSE_PROGRESS_LOADED=1

source "$(dirname "${BASH_SOURCE[0]}")/progress-reporter.sh"

# Run docker compose with progress reporting
docker_compose_with_progress() {
    local action="$1"  # up, down, etc.
    shift
    local args=("$@")
    
    case "$action" in
        "up")
            # Count containers/volumes/networks to create
            local compose_file="${args[-1]}"
            if [[ -f "$compose_file" ]]; then
                # Try to parse compose file for resource counts
                local container_count=$(yq eval '.services | length' "$compose_file" 2>/dev/null || echo 1)
                local volume_count=$(yq eval '.volumes | length' "$compose_file" 2>/dev/null || echo 0)
                local network_count=$(yq eval '.networks | length' "$compose_file" 2>/dev/null || echo 0)
            else
                local container_count=1
                local volume_count=0
                local network_count=0
            fi
            
            # Run docker compose and parse output
            docker compose "${args[@]}" 2>&1 | while IFS= read -r line; do
                case "$line" in
                    *"Network"*"Created"*)
                        progress_subtask_done "Network created"
                        ;;
                    *"Volume"*"Created"*)
                        volume_name=$(echo "$line" | grep -oP 'Volume "\K[^"]+')
                        progress_subtask_done "Volume created${volume_name:+ ($volume_name)}"
                        ;;
                    *"Container"*"Started"*)
                        container_name=$(echo "$line" | grep -oP 'Container \K[^ ]+')
                        progress_subtask_done "Container started${container_name:+ ($container_name)}"
                        ;;
                    *"Container"*"Created"*)
                        container_name=$(echo "$line" | grep -oP 'Container \K[^ ]+')
                        progress_subtask_done "Container created${container_name:+ ($container_name)}"
                        ;;
                esac
            done
            ;;
            
        "down")
            # Run docker compose and parse output
            docker compose "${args[@]}" 2>&1 | while IFS= read -r line; do
                case "$line" in
                    *"Container"*"Removed"*)
                        progress_subtask_done "Container removed"
                        ;;
                    *"Volume"*"Removed"*)
                        # Count volumes being removed
                        if [[ "$line" =~ "Volume.*Removed" ]]; then
                            volume_count=$(echo "$line" | grep -o "Volume" | wc -l)
                            progress_subtask_done "Volumes removed ($volume_count)"
                        fi
                        ;;
                    *"Network"*"Removed"*)
                        progress_subtask_done "Network removed"
                        ;;
                esac
            done
            ;;
            
        "build")
            # For build, track the build steps
            local current_step=""
            docker compose "${args[@]}" 2>&1 | while IFS= read -r line; do
                case "$line" in
                    "#"*"[internal]"*)
                        step_name=$(echo "$line" | sed -n 's/.*\[internal\] \(.*\)/\1/p')
                        [[ -n "$step_name" ]] && progress_update "."
                        ;;
                    "#"*"["*"/"*"]"*)
                        # Step progress like [1/10]
                        step_info=$(echo "$line" | grep -oP '\[\d+/\d+\]')
                        step_desc=$(echo "$line" | sed 's/.*\] //')
                        if [[ "$step_desc" != "$current_step" ]]; then
                            if [[ -n "$current_step" ]]; then
                                progress_done
                            fi
                            current_step="$step_desc"
                            progress_task "$step_desc $step_info" true
                        else
                            progress_update "."
                        fi
                        ;;
                    *"CACHED"*)
                        progress_update " (cached)"
                        ;;
                    *"DONE"*)
                        if [[ -n "$current_step" ]]; then
                            progress_done
                            current_step=""
                        fi
                        ;;
                    *"naming to"*)
                        image_name=$(echo "$line" | grep -oP 'naming to \K.*')
                        progress_subtask_done "Image built${image_name:+ ($image_name)}"
                        ;;
                esac
            done
            ;;
    esac
}

# Parse Docker build output with progress
docker_build_with_progress() {
    local current_step=""
    local total_steps=""
    
    docker build "$@" 2>&1 | while IFS= read -r line; do
        case "$line" in
            "#"*"["*"/"*"]"*)
                # Extract step number and total
                if [[ "$line" =~ \[([0-9]+)/([0-9]+)\] ]]; then
                    current_num="${BASH_REMATCH[1]}"
                    total_num="${BASH_REMATCH[2]}"
                    step_desc=$(echo "$line" | sed 's/.*\] //' | cut -d' ' -f1-5)
                    
                    if [[ "$step_desc" != "$current_step" ]]; then
                        if [[ -n "$current_step" ]]; then
                            progress_done
                        fi
                        current_step="$step_desc"
                        progress_task "$step_desc ($current_num/$total_num)" true
                    fi
                fi
                ;;
            *"CACHED"*)
                progress_update " (cached)"
                ;;
            *"DONE"*)
                if [[ -n "$current_step" ]]; then
                    progress_done
                    current_step=""
                fi
                ;;
            *"naming to"*|*"writing image"*)
                if [[ -n "$current_step" ]]; then
                    progress_done
                    current_step=""
                fi
                ;;
        esac
    done
}

# Export functions
export -f docker_compose_with_progress docker_build_with_progress