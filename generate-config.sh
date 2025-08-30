#!/bin/bash
# Config Generator - Create VM configurations by composing services
# Usage: ./generate-config.sh [--services service1,service2] [--ports start] [--name project] [output-file]

set -e
set -u

# Get script directory early for importing utilities
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Source temporary file utilities for secure temp file handling
source "$SCRIPT_DIR/shared/temporary-file-utils.sh"

# Set up proper cleanup handlers
setup_temp_file_handlers

# Check for required tools
if ! command -v yq &> /dev/null; then
    echo "❌ Error: yq is not installed. This tool is required for YAML processing."
    echo ""
    echo "📦 To install yq (mikefarah/yq v4+):"
    echo "   sudo apt remove yq 2>/dev/null || true"
    echo "   sudo wget -qO /usr/local/bin/yq https://github.com/mikefarah/yq/releases/latest/download/yq_linux_amd64"
    echo "   sudo chmod +x /usr/local/bin/yq"
    echo ""
    echo "📦 To install yq on macOS:"
    echo "   brew install yq"
    echo ""
    echo "📦 To install yq on other systems:"
    echo "   Visit: https://github.com/mikefarah/yq/releases"
    echo ""
    exit 1
fi


# Default values
DEFAULT_CONFIG="$SCRIPT_DIR/vm.yaml"
SERVICES=""
PORTS=""
PROJECT_NAME=""
OUTPUT_FILE="vm.yaml"

# Available services (discovered from configs/services/)
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
            echo "  $0 --name frontend-app my-frontend.yaml"
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

# Create a secure working copy of the base config
base_config_temp="$(create_temp_file "vm-config.XXXXXX")"
cp "$DEFAULT_CONFIG" "$base_config_temp"

# Apply service configurations by merging individual service YAML files
# Each service in the comma-separated list is validated and merged into base config
if [[ -n "$SERVICES" ]]; then
    # Split services by comma and process each
    IFS=',' read -ra service_list <<< "$SERVICES"
    
    for service in "${service_list[@]}"; do
        # Trim whitespace
        service="$(echo "$service" | xargs)"
        
        # Validate service exists
        if [[ ! " $AVAILABLE_SERVICES " =~ \ ${service}\  ]]; then
            echo "❌ Unknown service: $service" >&2
            echo "Available services: $AVAILABLE_SERVICES" >&2
            exit 1
        fi
        
        # Load service configuration
        service_config_file="$SCRIPT_DIR/configs/services/${service}.yaml"
        if [[ ! -f "$service_config_file" ]]; then
            echo "❌ Service configuration not found: $service_config_file" >&2
            exit 1
        fi
        
        # Extract the specific service configuration and merge it using yq
        service_value="$(yq ".services.$service" "$service_config_file")"
        
        # Use yq to merge the service configuration into the base config
        base_config_new="$(create_temp_file "vm-config-new.XXXXXX")"
        if ! echo "$service_value" | yq -p json -o yaml . | yq eval-all 'select(fileIndex == 0).services["'"$service"'"] = select(fileIndex == 1) | select(fileIndex == 0)' "$base_config_temp" - > "$base_config_new"; then
            echo "❌ Error: Failed to merge service configuration for: $service" >&2
            exit 1
        fi
        mv "$base_config_new" "$base_config_temp"
        untrack_temp_file "$base_config_new"  # No longer needed since content moved
    done
fi

# Helper function for safe yq operations
safe_yq_update() {
    local temp_file="$1"
    local yq_expression="$2"
    local error_message="$3"
    
    local temp_output
    temp_output="$(create_temp_file "yq-update.XXXXXX")"
    
    if ! yq "$yq_expression" "$temp_file" -o yaml > "$temp_output"; then
        echo "❌ Error: $error_message" >&2
        exit 1
    fi
    
    mv "$temp_output" "$temp_file"
    untrack_temp_file "$temp_output"  # No longer needed since content moved
}

# Apply project name and generate derived values (hostname, username)
# Updates project.name, project.hostname, and terminal.username fields
if [[ -n "$PROJECT_NAME" ]]; then
    safe_yq_update "$base_config_temp" ".project.name = \"$PROJECT_NAME\"" "Failed to set project name"
    safe_yq_update "$base_config_temp" ".project.hostname = \"dev.$PROJECT_NAME.local\"" "Failed to set project hostname"
    safe_yq_update "$base_config_temp" ".terminal.username = \"$PROJECT_NAME-dev\"" "Failed to set terminal username"
fi

# Apply port configuration starting from specified base port
# Allocates 10 sequential ports for different services (web, api, databases)
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
    
    # Set port configuration using safe yq operations
    safe_yq_update "$base_config_temp" ".ports.web = $web_port" "Failed to set web port"
    safe_yq_update "$base_config_temp" ".ports.api = $api_port" "Failed to set API port"
    safe_yq_update "$base_config_temp" ".ports.postgresql = $postgres_port" "Failed to set PostgreSQL port"
    safe_yq_update "$base_config_temp" ".ports.redis = $redis_port" "Failed to set Redis port"
    safe_yq_update "$base_config_temp" ".ports.mongodb = $mongodb_port" "Failed to set MongoDB port"
fi

# Auto-generate project name from current directory when not specified
# Uses basename of current working directory for project naming
if [[ -z "$PROJECT_NAME" ]]; then
    dir_name="$(basename "$(pwd)")"
    safe_yq_update "$base_config_temp" ".project.name = \"$dir_name\"" "Failed to set auto-generated project name"
    safe_yq_update "$base_config_temp" ".project.hostname = \"dev.$dir_name.local\"" "Failed to set auto-generated project hostname"
    safe_yq_update "$base_config_temp" ".terminal.username = \"$dir_name-dev\"" "Failed to set auto-generated terminal username"
fi

# Write final configuration
if cp "$base_config_temp" "$OUTPUT_FILE"; then
    project_name="$(yq '.project.name' "$OUTPUT_FILE")"
    echo "✅ Generated configuration for project: $project_name"
    echo "📍 Configuration file: $OUTPUT_FILE"
    
    # Show enabled services using yq
    enabled_services="$(yq '.services | to_entries[] | select(.value.enabled == true) | .key' "$OUTPUT_FILE" 2>/dev/null | tr '\n' ' ' || echo "none")"
    if [[ "$enabled_services" != "none" ]]; then
        echo "🔧 Enabled services: $enabled_services"
    fi
    
    # Show port allocations using yq
    ports="$(yq '.ports // {} | to_entries[] | .key + ":" + (.value | tostring)' "$OUTPUT_FILE" 2>/dev/null | tr '\n' ' ' || echo "")"
    if [[ -n "$ports" ]]; then
        echo "🔌 Port allocations: $ports"
    fi
    
    echo ""
    echo "Next steps:"
    echo "  1. Review and customize $OUTPUT_FILE as needed"
    echo "  2. Run \"vm create\" to start your development environment"
    
    # Temporary files will be cleaned up automatically by trap handlers
else
    echo "❌ Failed to generate configuration" >&2
    exit 1
fi