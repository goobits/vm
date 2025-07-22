#!/bin/bash
# Config Generator - Create VM configurations by composing services
# Usage: ./generate-config.sh [--services service1,service2] [--ports start] [--name project] [output-file]

set -e

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

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

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

# Create a working copy of the base config
base_config_temp="$(mktemp)"
cp "$DEFAULT_CONFIG" "$base_config_temp"

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
        service_config_file="$SCRIPT_DIR/configs/services/${service}.yaml"
        if [[ ! -f "$service_config_file" ]]; then
            echo "❌ Service configuration not found: $service_config_file" >&2
            exit 1
        fi
        
        # Extract the specific service configuration and merge it using yq
        service_value="$(yq -r ".services.$service" "$service_config_file")"
        
        # Use yq to merge the service configuration into the base config
        echo "$service_value" | yq -y . | yq -s '.[0].services["'$service'"] = .[1] | .[0]' "$base_config_temp" - > "${base_config_temp}.new"
        mv "${base_config_temp}.new" "$base_config_temp"
    done
fi

# Apply project name if specified
if [[ -n "$PROJECT_NAME" ]]; then
    yq -y ".project.name = \"$PROJECT_NAME\"" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".project.hostname = \"dev.$PROJECT_NAME.local\"" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".terminal.username = \"$PROJECT_NAME-dev\"" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
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
    
    # Set port configuration using yq
    yq -y ".ports.web = $web_port" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".ports.api = $api_port" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".ports.postgresql = $postgres_port" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".ports.redis = $redis_port" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".ports.mongodb = $mongodb_port" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
fi

# Auto-generate project name from directory if not specified
if [[ -z "$PROJECT_NAME" ]]; then
    dir_name="$(basename "$(pwd)")"
    yq -y ".project.name = \"$dir_name\"" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".project.hostname = \"dev.$dir_name.local\"" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
    yq -y ".terminal.username = \"$dir_name-dev\"" "$base_config_temp" > "${base_config_temp}.tmp" && mv "${base_config_temp}.tmp" "$base_config_temp"
fi

# Write final configuration
if cp "$base_config_temp" "$OUTPUT_FILE"; then
    project_name="$(yq -r '.project.name' "$OUTPUT_FILE")"
    echo "✅ Generated configuration for project: $project_name"
    echo "📍 Configuration file: $OUTPUT_FILE"
    
    # Show enabled services using yq
    enabled_services="$(yq -r '.services | to_entries[] | select(.value.enabled == true) | .key' "$OUTPUT_FILE" 2>/dev/null | tr '\n' ' ' || echo "none")"
    if [[ "$enabled_services" != "none" ]]; then
        echo "🔧 Enabled services: $enabled_services"
    fi
    
    # Show port allocations using yq
    ports="$(yq -r '.ports // {} | to_entries[] | .key + ":" + (.value | tostring)' "$OUTPUT_FILE" 2>/dev/null | tr '\n' ' ' || echo "")"
    if [[ -n "$ports" ]]; then
        echo "🔌 Port allocations: $ports"
    fi
    
    echo ""
    echo "Next steps:"
    echo "  1. Review and customize $OUTPUT_FILE as needed"
    echo "  2. Run \"vm create\" to start your development environment"
    
    # Clean up temporary file
    rm -f "$base_config_temp"
else
    echo "❌ Failed to generate configuration" >&2
    rm -f "$base_config_temp"
    exit 1
fi