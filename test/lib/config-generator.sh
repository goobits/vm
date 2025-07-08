#!/bin/bash
# Configuration Generator - Create test configurations programmatically

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_DIR="$(dirname "$SCRIPT_DIR")/configs"

# Load the default vm.json as base template
DEFAULT_CONFIG="/workspace/vm.json"

# Generate a config file with overrides
generate_config() {
    local name="$1"
    local overrides="$2"
    local output_file="${3:-$CONFIG_DIR/$name.json}"
    
    # Ensure config directory exists
    mkdir -p "$(dirname "$output_file")"
    
    # If overrides is a file, read it
    if [ -f "$overrides" ]; then
        overrides=$(cat "$overrides")
    fi
    
    # Use jq to merge default config with overrides
    if [ -n "$overrides" ]; then
        jq -s '.[0] * .[1]' "$DEFAULT_CONFIG" <(echo "$overrides") > "$output_file"
    else
        cp "$DEFAULT_CONFIG" "$output_file"
    fi
    
    echo "$output_file"
}

# Generate minimal configuration
generate_minimal_config() {
    local output="${1:-$CONFIG_DIR/minimal.json}"
    
    cat > "$output" << 'EOF'
{
    "$schema": "./vm.schema.json",
    "provider": "docker",
    "project": {
        "name": "test-minimal",
        "hostname": "dev.test-minimal.local",
        "workspace_path": "/workspace"
    },
    "vm": {
        "user": "vagrant"
    },
    "services": {},
    "npm_packages": [],
    "cargo_packages": [],
    "pip_packages": [],
    "aliases": {},
    "environment": {},
    "terminal": {
        "emoji": "🧪",
        "username": "test-min"
    }
}
EOF
    
    echo "$output"
}

# Generate config with specific service enabled
generate_service_config() {
    local service="$1"
    local enabled="${2:-true}"
    local output="${3:-$CONFIG_DIR/services/$service.json}"
    
    mkdir -p "$CONFIG_DIR/services"
    
    local overrides=$(cat << EOF
{
    "project": {
        "name": "test-$service"
    },
    "services": {
        "postgresql": {"enabled": false},
        "redis": {"enabled": false},
        "mongodb": {"enabled": false},
        "docker": {"enabled": false},
        "$service": {"enabled": $enabled}
    }
}
EOF
)
    
    generate_config "service-$service" "$overrides" "$output"
}

# Generate config with specific packages
generate_package_config() {
    local package_type="$1"  # npm_packages, cargo_packages, pip_packages
    local packages="$2"      # JSON array of packages
    local output="${3:-$CONFIG_DIR/languages/$package_type.json}"
    
    mkdir -p "$CONFIG_DIR/languages"
    
    local overrides=$(cat << EOF
{
    "project": {
        "name": "test-${package_type%_packages}"
    },
    "services": {},
    "$package_type": $packages
}
EOF
)
    
    generate_config "package-$package_type" "$overrides" "$output"
}

# Generate config with custom aliases
generate_alias_config() {
    local output="${1:-$CONFIG_DIR/aliases.json}"
    
    local overrides=$(cat << EOF
{
    "project": {
        "name": "test-aliases"
    },
    "aliases": {
        "hello": "echo 'Hello from alias'",
        "ll": "ls -la",
        "testcmd": "echo 'Test command executed'"
    },
    "environment": {
        "TEST_VAR": "test_value",
        "CUSTOM_PATH": "/custom/path"
    }
}
EOF
)
    
    generate_config "aliases" "$overrides" "$output"
}

# Generate config with port mappings
generate_port_config() {
    local output="${1:-$CONFIG_DIR/ports.json}"
    
    local overrides=$(cat << EOF
{
    "project": {
        "name": "test-ports"
    },
    "ports": {
        "web": 8080,
        "api": 3000,
        "custom": 9999
    },
    "services": {
        "postgresql": {
            "enabled": true,
            "port": 5433
        }
    }
}
EOF
)
    
    generate_config "ports" "$overrides" "$output"
}

# Generate all test configurations
generate_all_configs() {
    echo "Generating test configurations..."
    
    # Minimal config
    generate_minimal_config
    echo "✓ Generated minimal config"
    
    # Service configs
    for service in postgresql redis mongodb docker; do
        generate_service_config "$service" true
        echo "✓ Generated $service enabled config"
        
        generate_service_config "$service" false "$CONFIG_DIR/services/${service}-disabled.json"
        echo "✓ Generated $service disabled config"
    done
    
    # Language package configs
    generate_package_config "npm_packages" '["prettier", "eslint"]'
    echo "✓ Generated npm packages config"
    
    generate_package_config "cargo_packages" '["ripgrep", "tokei"]'
    echo "✓ Generated cargo packages config"
    
    generate_package_config "pip_packages" '["black", "pytest"]'
    echo "✓ Generated pip packages config"
    
    # Other configs
    generate_alias_config
    echo "✓ Generated aliases config"
    
    generate_port_config
    echo "✓ Generated ports config"
    
    echo "All configurations generated in: $CONFIG_DIR"
}