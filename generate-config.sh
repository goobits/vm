#!/bin/bash
# Config Generator - Create VM configurations by composing services
# Usage: ./generate-config.sh [--services service1,service2] [--ports start] [--name project] [output-file]

set -e

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Default values
DEFAULT_CONFIG="$SCRIPT_DIR/vm.json"
SERVICES=""
PORTS=""
PROJECT_NAME=""
OUTPUT_FILE="vm.json"

# Available services (discovered from test/configs/services/)
AVAILABLE_SERVICES="postgresql redis mongodb docker vm"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --services)
            SERVICES="$2"
            shift 2
            ;;
        --ports)
            PORTS="$2"
            shift 2
            ;;
        --name)
            PROJECT_NAME="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [options] [output-file]"
            echo ""
            echo "Options:"
            echo "  --services <list>    Comma-separated list of services to enable"
            echo "  --ports <start>      Starting port number (allocates 10 ports)"
            echo "  --name <name>        Project name"
            echo ""
            echo "Available services: $AVAILABLE_SERVICES"
            echo ""
            echo "Examples:"
            echo "  $0 --services postgresql,redis"
            echo "  $0 --services postgresql --ports 3020 --name my-app"
            echo "  $0 --name frontend-app my-frontend.json"
            exit 0
            ;;
        --*)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
        *)
            OUTPUT_FILE="$1"
            shift
            ;;
    esac
done

# Check if output file already exists
if [[ -f "$OUTPUT_FILE" ]]; then
    echo "❌ Configuration file already exists: $OUTPUT_FILE" >&2
    echo "Remove the existing file or specify a different output location." >&2
    exit 1
fi

# Load base configuration
if [[ ! -f "$DEFAULT_CONFIG" ]]; then
    echo "❌ Default configuration not found: $DEFAULT_CONFIG" >&2
    exit 1
fi

base_config="$(cat "$DEFAULT_CONFIG")"

# Apply services if specified
if [[ -n "$SERVICES" ]]; then
    # Split services by comma and process each
    IFS=',' read -ra service_list <<< "$SERVICES"
    
    for service in "${service_list[@]}"; do
        # Trim whitespace
        service="$(echo "$service" | xargs)"
        
        # Validate service exists
        if [[ ! " $AVAILABLE_SERVICES " =~ " $service " ]]; then
            echo "❌ Unknown service: $service" >&2
            echo "Available services: $AVAILABLE_SERVICES" >&2
            exit 1
        fi
        
        # Load service configuration
        service_config_file="$SCRIPT_DIR/test/configs/services/${service}.json"
        if [[ ! -f "$service_config_file" ]]; then
            echo "❌ Service configuration not found: $service_config_file" >&2
            exit 1
        fi
        
        service_config="$(cat "$service_config_file")"
        
        # Only merge specific service, not overwrite all services
        base_config="$(echo "$base_config" | jq --argjson service_cfg "$service_config" --arg service_name "$service" '
            .services[$service_name] = $service_cfg.services[$service_name]
        ')"
    done
fi

# Apply project name if specified
if [[ -n "$PROJECT_NAME" ]]; then
    base_config="$(echo "$base_config" | jq --arg name "$PROJECT_NAME" '
        .project.name = $name |
        .project.hostname = "dev." + $name + ".local" |
        .terminal.username = $name + "-dev"
    ')"
fi

# Apply port configuration if specified
if [[ -n "$PORTS" ]]; then
    # Validate port number
    if ! [[ "$PORTS" =~ ^[0-9]+$ ]] || [[ "$PORTS" -lt 1024 ]] || [[ "$PORTS" -gt 65535 ]]; then
        echo "❌ Invalid port number: $PORTS (must be between 1024-65535)" >&2
        exit 1
    fi
    
    # Generate port allocation (10 ports starting from specified number)
    web_port="$PORTS"
    api_port="$((PORTS + 1))"
    postgres_port="$((PORTS + 5))"
    redis_port="$((PORTS + 6))"
    mongodb_port="$((PORTS + 7))"
    
    base_config="$(echo "$base_config" | jq --argjson web "$web_port" --argjson api "$api_port" --argjson pg "$postgres_port" --argjson redis "$redis_port" --argjson mongo "$mongodb_port" '
        .ports = {
            "web": $web,
            "api": $api,
            "postgresql": $pg,
            "redis": $redis,
            "mongodb": $mongo
        }
    ')"
fi

# Auto-generate project name from directory if not specified
if [[ -z "$PROJECT_NAME" ]]; then
    dir_name="$(basename "$(pwd)")"
    base_config="$(echo "$base_config" | jq --arg name "$dir_name" '
        .project.name = $name |
        .project.hostname = "dev." + $name + ".local" |
        .terminal.username = $name + "-dev"
    ')"
fi

# Write final configuration
if echo "$base_config" | jq . > "$OUTPUT_FILE"; then
    project_name="$(echo "$base_config" | jq -r '.project.name')"
    echo "✅ Generated configuration for project: $project_name"
    echo "📍 Configuration file: $OUTPUT_FILE"
    
    # Show enabled services
    enabled_services="$(echo "$base_config" | jq -r '.services | to_entries[] | select(.value.enabled == true) | .key' 2>/dev/null | tr '\n' ' ' || echo "none")"
    if [[ "$enabled_services" != "none" ]]; then
        echo "🔧 Enabled services: $enabled_services"
    fi
    
    # Show port allocations
    ports="$(echo "$base_config" | jq -r '.ports // {} | to_entries[] | .key + ":" + (.value | tostring)' 2>/dev/null | tr '\n' ' ' || echo "")"
    if [[ -n "$ports" ]]; then
        echo "🔌 Port allocations: $ports"
    fi
    
    echo ""
    echo "Next steps:"
    echo "  1. Review and customize $OUTPUT_FILE as needed"
    echo "  2. Run \"vm up\" to start your development environment"
else
    echo "❌ Failed to generate configuration" >&2
    exit 1
fi