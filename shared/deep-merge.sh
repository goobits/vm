#!/bin/bash
# VM Configuration Deep Merge - Rust Implementation
# Purpose: Merge YAML configurations using fast Rust processor

set -e
set -u

# Get script directory for finding VM_CONFIG binary
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VM_TOOL_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Initialize VM_CONFIG path directly
VM_CONFIG="$VM_TOOL_DIR/rust/target/release/vm-config"

# Merge project configuration with default configuration files
# Args: default_config_path (YAML file), project_config_path (YAML file)
# Returns: Merged YAML configuration using Rust vm-config processor
merge_project_config() {
    local default_config_path="$1"
    local project_config_path="$2"

    # Always use Rust config processor
    if [[ ! -f "$default_config_path" ]]; then
        echo "❌ Default configuration not found: $default_config_path" >&2
        return 1
    fi

    if [[ ! -f "$project_config_path" ]]; then
        echo "❌ Project configuration not found: $project_config_path" >&2
        return 1
    fi

    # Use Rust implementation for fast, reliable merging
    "$VM_CONFIG" process \
        --defaults "$default_config_path" \
        --config "$project_config_path" \
        --format yaml
}

# Main function for command line usage
# Args: default_config_path, project_config_path
# Validates arguments and calls merge_project_config
main() {
    if [[ $# -ne 2 ]]; then
        echo "Usage: $0 <default-config.yaml> <project-config.yaml>" >&2
        echo "Outputs merged configuration to stdout" >&2
        return 1
    fi

    merge_project_config "$1" "$2"
}

# Run main if script is executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi