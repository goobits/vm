#!/bin/bash
# Config Generator - Create VM configurations by composing services
# Usage: ./generate-config.sh [--services service1,service2] [--ports start] [--name project] [output-file]

set -e
set -u

# Get script directory early for importing utilities
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Source platform utilities
source "$SCRIPT_DIR/shared/platform-utils.sh"

# Source temporary file utilities for secure temp file handling
source "$SCRIPT_DIR/shared/temporary-file-utils.sh"

# Set up proper cleanup handlers
setup_temp_file_handlers

# Check for required tools - vm-config binary
VM_CONFIG_BINARY="$SCRIPT_DIR/rust/vm-config/target/release/vm-config"
if [[ ! -x "$VM_CONFIG_BINARY" ]]; then
    echo "❌ Error: vm-config binary is not available."
    echo ""
    echo "📦 To build vm-config:"
    echo "   cd $SCRIPT_DIR/rust/vm-config"
    echo "   cargo build --release"
    echo ""
    echo "Or run the installer: ./install.sh"
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
        
        # Merge service configuration using vm-config
        base_config_new="$(create_temp_file "vm-config-new.XXXXXX")"
        if ! "$VM_CONFIG_BINARY" merge --base "$base_config_temp" --overlay "$service_config_file" -f yaml > "$base_config_new"; then
            echo "❌ Error: Failed to merge service configuration for: $service" >&2
            exit 1
        fi
        mv "$base_config_new" "$base_config_temp"
        untrack_temp_file "$base_config_new"  # No longer needed since content moved
    done
fi

# Helper function for safe vm-config operations (simplified)
safe_config_update() {
    local temp_file="$1"
    local field_path="$2"
    local new_value="$3"

    # Simple approach: append field to config
    echo "${field_path}: ${new_value}" >> "$temp_file"
}

# Apply project name and generate derived values (hostname, username)
# Updates project.name, project.hostname, and terminal.username fields
if [[ -n "$PROJECT_NAME" ]]; then
    # Create project override file for merging
    project_override="$(create_temp_file "project-override.XXXXXX")"
    cat > "$project_override" <<EOF
project:
  name: $PROJECT_NAME
  hostname: dev.$PROJECT_NAME.local
terminal:
  username: $PROJECT_NAME-dev
EOF

    # Merge project configuration using vm-config
    base_config_new="$(create_temp_file "vm-config-project.XXXXXX")"
    if ! "$VM_CONFIG_BINARY" merge --base "$base_config_temp" --overlay "$project_override" -f yaml > "$base_config_new"; then
        echo "❌ Error: Failed to merge project configuration" >&2
        exit 1
    fi
    mv "$base_config_new" "$base_config_temp"
    untrack_temp_file "$base_config_new"
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
    
    # Set port configuration using vm-config merge
    port_override="$(create_temp_file "port-override.XXXXXX")"
    cat > "$port_override" <<EOF
ports:
  web: $web_port
  api: $api_port
  postgresql: $postgres_port
  redis: $redis_port
  mongodb: $mongodb_port
EOF

    # Merge port configuration using vm-config
    base_config_new="$(create_temp_file "vm-config-ports.XXXXXX")"
    if ! "$VM_CONFIG_BINARY" merge --base "$base_config_temp" --overlay "$port_override" -f yaml > "$base_config_new"; then
        echo "❌ Error: Failed to merge port configuration" >&2
        exit 1
    fi
    mv "$base_config_new" "$base_config_temp"
    untrack_temp_file "$base_config_new"
fi

# Auto-generate project name from current directory when not specified
# Uses basename of current working directory for project naming
if [[ -z "$PROJECT_NAME" ]]; then
    dir_name="$(basename "$(pwd)")"
    # Create auto project override file for merging
    auto_project_override="$(create_temp_file "auto-project-override.XXXXXX")"
    cat > "$auto_project_override" <<EOF
project:
  name: $dir_name
  hostname: dev.$dir_name.local
terminal:
  username: $dir_name-dev
EOF

    # Merge auto project configuration using vm-config
    base_config_new="$(create_temp_file "vm-config-auto-project.XXXXXX")"
    if ! "$VM_CONFIG_BINARY" merge --base "$base_config_temp" --overlay "$auto_project_override" -f yaml > "$base_config_new"; then
        echo "❌ Error: Failed to merge auto project configuration" >&2
        exit 1
    fi
    mv "$base_config_new" "$base_config_temp"
    untrack_temp_file "$base_config_new"
fi

# Write final configuration
if cp "$base_config_temp" "$OUTPUT_FILE"; then
    project_name="$("$VM_CONFIG_BINARY" query "$OUTPUT_FILE" "project.name" --raw)"
    echo "✅ Generated configuration for project: $project_name"
    echo "📍 Configuration file: $OUTPUT_FILE"
    
    # Show enabled services using vm-config
    enabled_services="$("$VM_CONFIG_BINARY" query "$OUTPUT_FILE" "services" --raw 2>/dev/null | grep -E "^\s*\w+:\s*$" | sed 's/:.*$//' | xargs || echo "none")"
    if [[ "$enabled_services" != "none" ]]; then
        echo "🔧 Enabled services: $enabled_services"
    fi
    
    # Show port allocations using vm-config
    ports="$("$VM_CONFIG_BINARY" query "$OUTPUT_FILE" "ports" --raw 2>/dev/null | grep -E "^\s*\w+:\s*[0-9]+\s*$" | sed 's/^\s*//' | xargs || echo "")"
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