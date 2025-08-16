#!/bin/bash
# VM Configuration Processor - Unified config loading and processing
# Purpose: Provide a single, shared configuration processing layer for both Docker and Vagrant
# Eliminates code duplication across vm.sh, Vagrantfile, and vm-temporary.sh

set -e
set -u

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

    yq -o json "$filter" "$file" 2>/dev/null || echo "null"
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
    
    # Get the VM tool's workspace directory to exclude it
    local vm_tool_workspace
    vm_tool_workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

    while [[ "$current_dir" != "/" ]]; do
        if [[ -f "$current_dir/vm.yaml" ]]; then
            # Skip the VM tool's own vm.yaml
            if [[ "$current_dir" == "$vm_tool_workspace" ]]; then
                current_dir="$(dirname "$current_dir")"
                continue
            fi
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
# PRESET DETECTION AND LOADING
# Functions for detecting partial configs and loading presets
#=============================================================================

# Check if a configuration is partial (missing required fields)
# Args: config_json
# Returns: 0 if partial, 1 if complete
is_partial_config() {
    local config="$1"

    # Check for required fields
    local project_name
    project_name="$(echo "$config" | jq -r '.project.name // empty' 2>/dev/null || echo "")"

    # If project.name is missing or empty, it's a partial config
    if [[ -z "$project_name" || "$project_name" == "null" ]]; then
        return 0  # partial
    fi

    # Check if config explicitly requests preset inheritance
    local use_preset
    use_preset="$(echo "$config" | jq -r '.use_preset // empty' 2>/dev/null || echo "")"
    if [[ -n "$use_preset" && "$use_preset" != "null" && "$use_preset" != "false" ]]; then
        return 0  # partial - wants preset
    fi

    # Check if config has a preset field
    local preset_name
    preset_name="$(echo "$config" | jq -r '.preset // empty' 2>/dev/null || echo "")"
    if [[ -n "$preset_name" && "$preset_name" != "null" ]]; then
        return 0  # partial - specifies preset
    fi

    return 1  # complete
}

# Get the preset name from a partial config or detect from project
# Args: config_json, project_dir
# Returns: preset name (e.g., "nodejs", "python", "base")
get_preset_name() {
    local config="$1"
    local project_dir="$2"

    # First check for forced preset from environment
    if [[ -n "${VM_FORCED_PRESET:-}" ]]; then
        echo "$VM_FORCED_PRESET"
        return 0
    fi

    # Then check if config explicitly specifies a preset
    local preset_name
    preset_name="$(echo "$config" | jq -r '.preset // empty' 2>/dev/null || echo "")"
    if [[ -n "$preset_name" && "$preset_name" != "null" ]]; then
        echo "$preset_name"
        return 0
    fi

    # Otherwise, detect from project type
    if [[ -f "$CONFIG_SCRIPT_DIR/project-detector.sh" ]]; then
        source "$CONFIG_SCRIPT_DIR/project-detector.sh"
        local detected_type
        detected_type="$(detect_project_type "$project_dir")"

        # Handle multi-type projects by taking the first type
        if [[ "$detected_type" == multi:* ]]; then
            detected_type="${detected_type#multi:}"
            detected_type="${detected_type%% *}"
        fi

        echo "$detected_type"
    else
        echo "base"  # fallback to base preset
    fi
}

# Load preset configuration file
# Args: preset_name
# Returns: JSON configuration or empty object if not found
load_preset() {
    local preset_name="$1"
    local parent_dir
    parent_dir="$(dirname "$CONFIG_SCRIPT_DIR")"
    local preset_path="$parent_dir/configs/presets/${preset_name}.yaml"

    if [[ -f "$preset_path" ]]; then
        yq_json '.' "$preset_path" 2>/dev/null || echo "{}"
    else
        echo "{}"
    fi
}

# Extract schema defaults as JSON
# This function retrieves default values from the VM schema file to ensure
# all configurations have proper fallback values even when presets don't specify them
# Args: none (uses hardcoded schema path)
# Returns: JSON with schema defaults
get_schema_defaults() {
    local parent_dir
    parent_dir="$(dirname "$CONFIG_SCRIPT_DIR")"
    local schema_path="$parent_dir/vm.schema.yaml"

    if [[ ! -f "$schema_path" ]]; then
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG config-processor: Schema file not found: $schema_path" >&2
        fi
        echo "{}"
        return
    fi

    # Extract defaults from schema using validate-config.sh
    local validate_script="$parent_dir/validate-config.sh"
    if [[ -f "$validate_script" ]]; then
        local defaults
        if defaults="$("$validate_script" --extract-defaults "$schema_path" 2>&1)"; then
            echo "$defaults"
        else
            if [[ "${VM_DEBUG:-}" = "true" ]]; then
                echo "DEBUG config-processor: Failed to extract schema defaults: $defaults" >&2
            fi
            echo "{}"
        fi
    else
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG config-processor: Validate script not found: $validate_script" >&2
        fi
        echo "{}"
    fi
}

#=============================================================================
# ENHANCED CONFIG LOADING WITH PRESET SUPPORT
#=============================================================================

# Load configuration with full preset support
# This is the main configuration loading function that handles the complete preset chain:
# 1. Schema defaults (foundation)
# 2. Base preset (common development environment)
# 3. Detected/specified presets (language/framework specific)
# 4. User configuration (project-specific overrides)
# Args: config_path, project_dir
# Returns: Fully merged JSON configuration
load_config_with_presets() {
    local config_path="$1"
    local project_dir="${2:-$(pwd)}"

    # Debug output
    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "DEBUG config-processor: load_config_with_presets called" >&2
        echo "  config_path='$config_path'" >&2
        echo "  project_dir='$project_dir'" >&2
    fi

    # Step 1: Get schema defaults
    local schema_defaults
    schema_defaults="$(get_schema_defaults)"
    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "DEBUG config-processor: Schema defaults loaded" >&2
    fi

    # Step 2: Load base preset
    local base_preset
    base_preset="$(load_preset "base")"
    if [[ "${VM_DEBUG:-}" = "true" ]]; then
        echo "DEBUG config-processor: Base preset loaded" >&2
    fi

    # Step 3: Try to load user config if it exists
    local user_config="{}"
    local config_exists=false

    if [[ -n "$config_path" && "$config_path" != "__SCAN__" && -f "$config_path" ]]; then
        # Check if this is a JSON file before attempting to parse
        if [[ "$config_path" == *.json ]]; then
            echo "❌ Failed to parse configuration file: $config_path" >&2
            echo "" >&2
            echo "   JSON configs are no longer supported." >&2
            echo "" >&2
            echo "   To migrate your configuration, run:" >&2
            echo "     vm migrate --input $config_path" >&2
            echo "" >&2
            return 1
        fi
        user_config="$(yq_json '.' "$config_path" 2>/dev/null || echo "{}")"
        config_exists=true
    elif [[ "$config_path" == "__SCAN__" ]]; then
        # Scan for config
        local found_config
        found_config="$(find_vm_yaml_upwards "$project_dir")"
        if [[ -n "$found_config" ]]; then
            # Check if this is a JSON file before attempting to parse
            if [[ "$found_config" == *.json ]]; then
                echo "❌ Failed to parse configuration file: $found_config" >&2
                echo "" >&2
                echo "   JSON configs are no longer supported." >&2
                echo "" >&2
                echo "   To migrate your configuration, run:" >&2
                echo "     vm migrate --input $found_config" >&2
                echo "" >&2
                return 1
            fi
            user_config="$(yq_json '.' "$found_config" 2>/dev/null || echo "{}")"
            config_exists=true
        fi
    elif [[ -f "$project_dir/vm.yaml" ]]; then
        # Check if this is a JSON file before attempting to parse (though unlikely with .yaml extension)
        if [[ "$project_dir/vm.yaml" == *.json ]]; then
            echo "❌ Failed to parse configuration file: $project_dir/vm.yaml" >&2
            echo "" >&2
            echo "   JSON configs are no longer supported." >&2
            echo "" >&2
            echo "   To migrate your configuration, run:" >&2
            echo "     vm migrate --input $project_dir/vm.yaml" >&2
            echo "" >&2
            return 1
        fi
        user_config="$(yq_json '.' "$project_dir/vm.yaml" 2>/dev/null || echo "{}")"
        config_exists=true
    fi

    # Step 4: Determine if we need to load a detected preset
    local detected_preset="{}"
    if [[ "$config_exists" = "true" ]] && is_partial_config "$user_config"; then
        # Partial config - load detected preset
        local preset_name
        preset_name="$(get_preset_name "$user_config" "$project_dir")"
        if [[ "${VM_DEBUG:-}" = "true" ]]; then
            echo "DEBUG config-processor: Partial config detected, using preset: $preset_name" >&2
        fi
        if [[ "$preset_name" != "base" && "$preset_name" != "generic" ]]; then
            detected_preset="$(load_preset "$preset_name")"
        fi
    elif [[ "$config_exists" = "false" ]]; then
        # No config - use forced preset or detect from project
        local detected_type

        if [[ -n "${VM_FORCED_PRESET:-}" ]]; then
            detected_type="$VM_FORCED_PRESET"
            if [[ "${VM_DEBUG:-}" = "true" ]]; then
                echo "DEBUG config-processor: No config found, using forced preset: $detected_type" >&2
            fi
        else
            source "$CONFIG_SCRIPT_DIR/project-detector.sh"
            detected_type="$(detect_project_type "$project_dir")"

            # Handle multi-type projects
            if [[ "$detected_type" == multi:* ]]; then
                detected_type="${detected_type#multi:}"
                detected_type="${detected_type%% *}"
            fi

            if [[ "${VM_DEBUG:-}" = "true" ]]; then
                echo "DEBUG config-processor: No config found, detected type: $detected_type" >&2
            fi
        fi

        if [[ "$detected_type" != "generic" ]]; then
            detected_preset="$(load_preset "$detected_type")"
        fi
    fi

    # Step 5: Perform the merge chain
    # Order: Schema defaults -> Base preset -> Detected preset -> User config
    local merged_config="$schema_defaults"

    # Merge base preset
    if [[ "$base_preset" != "{}" ]]; then
        merged_config="$(deep_merge_bash "$merged_config" "$base_preset")"
    fi

    # Merge detected preset
    if [[ "$detected_preset" != "{}" ]]; then
        merged_config="$(deep_merge_bash "$merged_config" "$detected_preset")"
    fi

    # Merge user config (if exists)
    if [[ "$config_exists" = "true" && "$user_config" != "{}" ]]; then
        merged_config="$(deep_merge_bash "$merged_config" "$user_config")"
    fi

    # Add project name if missing (from directory name)
    local project_name
    project_name="$(echo "$merged_config" | jq -r '.project.name // empty' 2>/dev/null || echo "")"
    if [[ -z "$project_name" || "$project_name" == "null" ]]; then
        # Use sanitized directory name
        project_name="$(basename "$project_dir" | tr -cd '[:alnum:]')"
        if [[ -n "$project_name" ]]; then
            merged_config="$(echo "$merged_config" | jq --arg name "$project_name" '.project.name = $name')"
        fi
    fi

    echo "$merged_config"
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

    # Check if we should use preset-aware loading
    local use_presets="${VM_USE_PRESETS:-true}"

    if [[ "$use_presets" == "true" ]]; then
        # Use the new preset-aware loading
        load_config_with_presets "$config_path" "$original_dir"
    else
        # Fallback to original behavior for compatibility
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
    fi
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
        "load-with-presets")
            load_config_with_presets "$config_path"
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
        "is-partial")
            # Load config and check if it's partial
            local config
            if [[ -n "$config_path" && -f "$config_path" ]]; then
                config="$(yq_json '.' "$config_path" 2>/dev/null || echo "{}")"
            else
                config="{}"
            fi
            if is_partial_config "$config"; then
                echo "true"
                exit 0
            else
                echo "false"
                exit 1
            fi
            ;;
        "get-preset")
            # Get the preset name for a config/project
            local config="{}"
            if [[ -n "$config_path" && -f "$config_path" ]]; then
                config="$(yq_json '.' "$config_path" 2>/dev/null || echo "{}")"
            fi
            get_preset_name "$config" "$(pwd)"
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
            echo "  load             Load and merge configuration (with presets if enabled)"
            echo "  load-with-presets Load config with full preset support"
            echo "  validate         Validate configuration file"
            echo "  init             Initialize new configuration file"
            echo "  get-provider     Get provider from config"
            echo "  get-project-name Get project name from config"
            echo "  is-partial       Check if config is partial (needs presets)"
            echo "  get-preset       Get preset name for config/project"
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