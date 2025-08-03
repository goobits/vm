#!/bin/bash
# VM Configuration Processor - Unified config loading and processing
# Purpose: Provide a single, shared configuration processing layer for both Docker and Vagrant
# Eliminates code duplication across vm.sh, Vagrantfile, and vm-temporary.sh

set -e

# Get shared utilities directory
CONFIG_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source existing shared utilities
if [[ -f "$CONFIG_SCRIPT_DIR/deep-merge.sh" ]]; then
    source "$CONFIG_SCRIPT_DIR/deep-merge.sh"
fi

#=============================================================================
# YQ WRAPPER UTILITIES
# Extracted and unified from vm-temporary.sh
#=============================================================================

# Wrapper function for yq to handle different versions and provide consistent output
# This system has Python yq (kislyuk/yq) which outputs JSON, not raw strings
# Args: filter, file
# Returns: Raw string output (not JSON)
yq_raw() {
    local filter="$1"
    local file="$2"
    
    if [[ ! -f "$file" ]]; then
        echo ""
        return 1
    fi
    
    # Use yq to process YAML, then jq to extract raw values
    yq "$filter" "$file" 2>/dev/null | jq -r '.' 2>/dev/null || echo ""
}

# Alternative yq wrapper that preserves JSON output for complex operations
# Args: filter, file
# Returns: JSON output
yq_json() {
    local filter="$1"
    local file="$2"
    
    if [[ ! -f "$file" ]]; then
        echo "null"
        return 1
    fi
    
    yq "$filter" "$file" 2>/dev/null || echo "null"
}

#=============================================================================
# DEEP MERGE FUNCTIONALITY
# Converted from Ruby (Vagrantfile lines 8-16) to bash equivalent
#=============================================================================

# Deep merge two YAML/JSON configurations
# This is the bash equivalent of the Ruby deep_merge function from Vagrantfile
# Args: base_config_json, override_config_json
# Returns: Merged JSON configuration with override values taking precedence
deep_merge_bash() {
    local base_config="$1"
    local override_config="$2"
    
    # Use jq for deep merging - similar to Ruby's deep_merge behavior
    echo "$base_config" | jq --argjson override "$override_config" '
        def deep_merge(base; override):
            if (base | type) == "object" and (override | type) == "object" then
                # Merge objects recursively
                base + reduce (override | keys_unsorted[]) as $key ({};
                    if base | has($key) then
                        .[$key] = deep_merge(base[$key]; override[$key])
                    else
                        .[$key] = override[$key]
                    end)
            elif (base | type) == "array" and (override | type) == "array" then
                # For arrays, override completely (Ruby behavior)
                override
            else
                # For primitives, override wins (Ruby behavior)
                override
            end;
        
        deep_merge(.; $override)
    ' 2>/dev/null || echo "{}"
}

# Load and merge YAML configurations (file-based version)
# Args: base_config_path, override_config_path
# Returns: Merged JSON configuration
deep_merge_files() {
    local base_config_path="$1"
    local override_config_path="$2"
    
    # Validate inputs
    if [[ ! -f "$base_config_path" ]]; then
        echo "❌ Base configuration not found: $base_config_path" >&2
        return 1
    fi
    
    if [[ ! -f "$override_config_path" ]]; then
        echo "❌ Override configuration not found: $override_config_path" >&2
        return 1
    fi
    
    # Load configurations (convert YAML to JSON)
    local base_json
    if ! base_json="$(yq_json '.' "$base_config_path")"; then
        echo "❌ Invalid YAML in base config: $base_config_path" >&2
        return 1
    fi
    
    local override_json
    if ! override_json="$(yq_json '.' "$override_config_path")"; then
        echo "❌ Invalid YAML in override config: $override_config_path" >&2
        return 1
    fi
    
    # Perform deep merge
    deep_merge_bash "$base_json" "$override_json"
}

#=============================================================================
# CONFIG DISCOVERY AND LOADING
# Extracted from vm.sh and validate-config.sh
#=============================================================================

# Find vm.yaml upwards from directory (extracted from validate-config.sh)
# Args: start_directory
# Returns: Path to vm.yaml file if found
find_vm_yaml_upwards() {
    local start_dir="$1"
    local current_dir
    current_dir="$(cd "$start_dir" && pwd)"

    while [[ "$current_dir" != "/" ]]; do
        if [[ -f "$current_dir/vm.yaml" ]]; then
            echo "$current_dir/vm.yaml"
            return 0
        fi
        current_dir="$(dirname "$current_dir")"
    done

    # Check root directory
    if [[ -f "/vm.yaml" ]]; then
        echo "/vm.yaml"
        return 0
    fi

    return 1
}

# Get provider from config (extracted from vm.sh line 643-646)
# Args: config_json
# Returns: Provider name (defaults to "docker")
get_config_provider() {
    local config="$1"
    echo "$config" | jq -r '.provider // "docker"' 2>/dev/null || echo "docker"
}

# Extract project name from config
# Args: config_json
# Returns: Project name (sanitized to alphanumeric only)
get_config_project_name() {
    local config="$1"
    local project_name
    project_name="$(echo "$config" | jq -r '.project.name // empty' 2>/dev/null || echo "")"
    
    # Sanitize project name to alphanumeric only (matching vm.sh behavior)
    echo "$project_name" | tr -cd '[:alnum:]'
}

# Extract config value with fallback
# Args: config_json, jq_path, fallback_value
# Returns: Config value or fallback
extract_config_value() {
    local config="$1"
    local jq_path="$2"
    local fallback="${3:-}"
    
    local value
    value="$(echo "$config" | jq -r "$jq_path // empty" 2>/dev/null || echo "")"
    
    if [[ -z "$value" || "$value" == "null" ]]; then
        echo "$fallback"
    else
        echo "$value"
    fi
}

#=============================================================================
# UNIFIED CONFIG LOADING AND MERGING
# Integrates with existing validate-config.sh and provides unified interface
#=============================================================================

# Load and merge configuration using the existing validate-config.sh infrastructure
# This replaces the load_config function from vm.sh (lines 615-639)
# Args: config_path (optional, can be "__SCAN__" for upward scanning)
# Returns: Merged and validated JSON configuration
load_and_merge_config() {
    local config_path="$1"
    local original_dir="${2:-$(pwd)}"
    
    # Get the parent script directory to find validate-config.sh
    local parent_script_dir
    parent_script_dir="$(dirname "$CONFIG_SCRIPT_DIR")"
    local validate_script="$parent_script_dir/validate-config.sh"
    
    # Debug output if --debug flag is set
    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "DEBUG config-processor: load_and_merge_config called with config_path='$config_path'" >&2
        echo "DEBUG config-processor: original_dir='$original_dir'" >&2
        echo "DEBUG config-processor: validate_script='$validate_script'" >&2
    fi
    
    # Check if validate-config.sh exists
    if [[ ! -f "$validate_script" ]]; then
        echo "❌ Configuration validator not found: $validate_script" >&2
        return 1
    fi
    
    local config_result
    if [[ -n "$config_path" ]]; then
        # Use custom config path or scan mode
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG config-processor: Running: cd '$original_dir' && '$validate_script' --get-config '$config_path'" >&2
        fi
        if ! config_result="$(cd "$original_dir" && "$validate_script" --get-config "$config_path" 2>&1)"; then
            echo "❌ Configuration loading failed: $config_result" >&2
            return 1
        fi
    else
        # Use default discovery logic
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG config-processor: Running: cd '$original_dir' && '$validate_script' --get-config" >&2
        fi
        if ! config_result="$(cd "$original_dir" && "$validate_script" --get-config 2>&1)"; then
            echo "❌ Configuration loading failed: $config_result" >&2
            return 1
        fi
    fi
    
    # Return the validated and merged configuration
    echo "$config_result"
}

# Validate configuration file using existing validation infrastructure
# Args: config_path
# Returns: 0 on success, 1 on failure
validate_config_file() {
    local config_path="$1"
    
    # Get the parent script directory to find validate-config.sh
    local parent_script_dir
    parent_script_dir="$(dirname "$CONFIG_SCRIPT_DIR")"
    local validate_script="$parent_script_dir/validate-config.sh"
    
    if [[ ! -f "$validate_script" ]]; then
        echo "❌ Configuration validator not found: $validate_script" >&2
        return 1
    fi
    
    if [[ -n "$config_path" ]]; then
        "$validate_script" --validate "$config_path"
    else
        "$validate_script" --validate
    fi
}

# Initialize new configuration file using existing infrastructure
# Args: config_path (optional)
# Returns: 0 on success, 1 on failure
init_config_file() {
    local config_path="$1"
    
    # Get the parent script directory to find validate-config.sh
    local parent_script_dir
    parent_script_dir="$(dirname "$CONFIG_SCRIPT_DIR")"
    local validate_script="$parent_script_dir/validate-config.sh"
    
    if [[ ! -f "$validate_script" ]]; then
        echo "❌ Configuration validator not found: $validate_script" >&2
        return 1
    fi
    
    if [[ -n "$config_path" ]]; then
        "$validate_script" --init "$config_path"
    else
        "$validate_script" --init
    fi
}

#=============================================================================
# CONVENIENCE FUNCTIONS
# High-level functions for common config operations
#=============================================================================

# Load config and extract provider in one call
# Args: config_path (optional)
# Returns: Provider name
get_provider() {
    local config_path="$1"
    local original_dir="${2:-$(pwd)}"
    
    local config
    if ! config="$(load_and_merge_config "$config_path" "$original_dir")"; then
        return 1
    fi
    
    get_config_provider "$config"
}

# Load config and extract project name in one call
# Args: config_path (optional)
# Returns: Project name
get_project_name() {
    local config_path="$1"
    local original_dir="${2:-$(pwd)}"
    
    local config
    if ! config="$(load_and_merge_config "$config_path" "$original_dir")"; then
        return 1
    fi
    
    get_config_project_name "$config"
}

# Check if config file exists using discovery logic
# Args: config_path (optional, "__SCAN__" for scanning)
# Returns: 0 if config exists, 1 if not found
config_exists() {
    local config_path="$1"
    
    if [[ "$config_path" == "__SCAN__" ]]; then
        # Use scanning logic
        find_vm_yaml_upwards "$(pwd)" >/dev/null 2>&1
    elif [[ -n "$config_path" ]]; then
        # Check specific path
        [[ -f "$config_path" ]]
    else
        # Check local vm.yaml
        [[ -f "$(pwd)/vm.yaml" ]]
    fi
}

#=============================================================================
# MAIN FUNCTION FOR COMMAND LINE USAGE
#=============================================================================

# Main function for standalone usage
# Args: command, config_path (optional)
main() {
    local command="${1:-}"
    local config_path="${2:-}"
    
    case "$command" in
        "load"|"load-config")
            load_and_merge_config "$config_path"
            ;;
        "validate")
            validate_config_file "$config_path"
            ;;
        "init")
            init_config_file "$config_path"
            ;;
        "get-provider")
            get_provider "$config_path"
            ;;
        "get-project-name")
            get_project_name "$config_path"
            ;;
        "exists")
            if config_exists "$config_path"; then
                echo "true"
                exit 0
            else
                echo "false"
                exit 1
            fi
            ;;
        "help"|"-h"|"--help"|"")
            echo "VM Configuration Processor - Unified config loading and processing"
            echo ""
            echo "Usage: $0 <command> [config-path]"
            echo ""
            echo "Commands:"
            echo "  load             Load and merge configuration"
            echo "  validate         Validate configuration file"
            echo "  init             Initialize new configuration file"
            echo "  get-provider     Get provider from config"
            echo "  get-project-name Get project name from config"
            echo "  exists           Check if config exists"
            echo "  help             Show this help"
            echo ""
            echo "Config path can be:"
            echo "  - Specific file path"
            echo "  - '__SCAN__' to scan upwards for vm.yaml"
            echo "  - Empty to use ./vm.yaml"
            echo ""
            ;;
        *)
            echo "❌ Unknown command: $command" >&2
            echo "Run '$0 help' for usage information" >&2
            exit 1
            ;;
    esac
}

# Run main if script is executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi