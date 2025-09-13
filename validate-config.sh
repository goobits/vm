#!/bin/bash
# VM Configuration Manager - Shell version
# Purpose: Load, merge, validate, and output final configuration using vm-config for YAML
# Usage: ./validate-config.sh [--validate] [--get-config] [--init] [config-path]

set -e
set -u

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Initialize Rust binary paths (these are bundled with the project)
VM_CONFIG="$SCRIPT_DIR/rust/target/release/vm-config"
VM_PORTS="$SCRIPT_DIR/rust/target/release/vm-ports"
VM_LINKS="$SCRIPT_DIR/rust/target/release/vm-links"

# Source shared platform utilities
source "$SCRIPT_DIR/shared/platform-utils.sh"

# Source shared deep merge utilities
source "$SCRIPT_DIR/shared/deep-merge.sh"

# Port management now handled by vm-ports binary

# Initialize variables
VALIDATE_FLAG=""
GET_CONFIG_FLAG=""
INIT_FLAG=""
CUSTOM_CONFIG_PATH=""

# Parse arguments
EXTRACT_DEFAULTS_PATH=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --validate)
            VALIDATE_FLAG="true"
            shift
            ;;
        --get-config)
            GET_CONFIG_FLAG="true"
            shift
            ;;
        --init)
            INIT_FLAG="true"
            shift
            ;;
        --extract-defaults)
            shift
            if [[ $# -eq 0 ]]; then
                echo "Error: --extract-defaults requires a schema file path" >&2
                exit 1
            fi
            EXTRACT_DEFAULTS_PATH="$1"
            shift
            ;;
        --*)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
        *)
            CUSTOM_CONFIG_PATH="$1"
            shift
            ;;
    esac
done


# Extract default values from YAML schema
extract_schema_defaults() {
    local schema_path="$1"

    if [[ ! -f "$schema_path" ]]; then
        echo "❌ Schema file not found: $schema_path" >&2
        return 1
    fi

    # Use the default vm.yaml file as the source of defaults
    # This is simpler and more reliable than parsing the JSON schema
    if [[ -f "$SCRIPT_DIR/vm.yaml" ]]; then
        cat "$SCRIPT_DIR/vm.yaml"
    else
        echo "❌ Default vm.yaml not found at $SCRIPT_DIR/vm.yaml" >&2
        return 1
    fi
}

# Find vm.yaml upwards from directory
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

# Initialize vm.yaml
initialize_vm_yaml() {
    local target_path="$1"
    local local_config_path
    
    if [[ -n "$target_path" ]]; then
        # Create directory if it doesn't exist
        mkdir -p "$target_path"
        local_config_path="$target_path/vm.yaml"
    else
        local_config_path="$(pwd)/vm.yaml"
    fi

    # Check if vm.yaml already exists
    if [[ -f "$local_config_path" ]]; then
        echo "❌ vm.yaml already exists at $local_config_path" >&2
        echo "Use --config to specify a different location or remove the existing file." >&2
        return 1
    fi

    # Load default configuration from schema
    local schema_path="$SCRIPT_DIR/vm.schema.yaml"
    if [[ ! -f "$schema_path" ]]; then
        echo "❌ Schema file not found at $schema_path" >&2
        return 1
    fi

    local default_config
    default_config="$(extract_schema_defaults "$schema_path")"
    local dir_name
    dir_name="$(basename "$(pwd)")"

    # Sanitize directory name for use as project name
    # Replace dots, spaces, and other invalid characters with hyphens
    # Then remove any consecutive hyphens and trim leading/trailing hyphens
    local sanitized_name
    sanitized_name="$(echo "$dir_name" | sed 's/[^a-zA-Z0-9_-]/-/g' | sed 's/--*/-/g' | sed 's/^-//;s/-$//')"

    # If the sanitized name is different, inform the user
    if [[ "$sanitized_name" != "$dir_name" ]]; then
        echo "📝 Note: Directory name '$dir_name' contains invalid characters for project names."
        echo "   Using sanitized name: '$sanitized_name'"
        echo ""
    fi

    # Customize config for this directory
    local customized_config
    # Complex config customization disabled (was using jq syntax)
    customized_config="$default_config"
    # Note: Complex config customization disabled for simplicity

    # Auto-suggest port range for the project
    local suggested_range
    if suggested_range="$("$VM_PORTS" suggest 10 2>/dev/null)"; then
        # Add port range to config
        customized_config="${customized_config}
port_range: $suggested_range"
        echo "🔢 Auto-suggested port range: $suggested_range"
        echo ""
    fi

    # Create a simplified config focused on the OS field
    cat > "$local_config_path" << EOF
# VM Configuration - Simple and powerful!
# Just specify the OS you want to run:

os: ubuntu  # Options: ubuntu, debian, alpine, macos (on Apple Silicon)

# Everything else is auto-configured based on your OS choice!
# - Ubuntu: 4GB RAM, 2 CPUs, Docker provider
# - macOS: 8GB RAM, 4 CPUs, Tart provider (Apple Silicon only)
# - Debian: 2GB RAM, 2 CPUs, Docker provider
# - Alpine: 1GB RAM, 1 CPU, Docker provider

# Optional: Override defaults by adding vm section
# vm:
#   memory: 8192  # Custom RAM in MB
#   cpus: 4       # Custom CPU cores

# Optional: Store VMs on external drive (for Tart provider)
# tart:
#   storage_path: /Volumes/ExternalSSD/VMs

# Project settings (auto-generated)
project:
  name: $sanitized_name
  hostname: dev.$sanitized_name.local

# Port range for this project
$(if [[ -n "$suggested_range" ]]; then echo "port_range: $suggested_range"; else echo "# port_range: 3000-3009  # Uncomment to reserve ports"; fi)
EOF

    if [[ -f "$local_config_path" ]]; then
        echo "✅ Created vm.yaml for project: $sanitized_name"
        echo "📍 Configuration file: $local_config_path"
        echo ""
        echo "🎯 The new simple way:"
        echo "   Just set 'os: ubuntu' (or macos, debian, alpine)"
        echo "   Everything else is auto-configured!"
        echo ""
        echo "Next steps:"
        echo "  1. Review vm.yaml (it's really simple now!)"
        echo "  2. Run \"vm create\" to start your environment"
        return 0
    else
        echo "❌ Failed to create vm.yaml" >&2
        return 1
    fi
}

# Load and merge configuration
load_and_merge_config() {
    local custom_config_path="$1"
    local local_config_path
    local_config_path="$(pwd)/vm.yaml"
    local config_file_to_load=""
    local config_dir_for_scan=""

    # Load default config from schema
    local schema_path="$SCRIPT_DIR/vm.schema.yaml"
    if [[ ! -f "$schema_path" ]]; then
        echo "❌ Schema file not found at $schema_path" >&2
        return 1
    fi

    local default_config
    default_config="$(extract_schema_defaults "$schema_path")"

    # Determine which config to load
    if [[ "$custom_config_path" == "__SCAN__" ]]; then
        # Scan upwards for vm.yaml
        if config_file_to_load="$(find_vm_yaml_upwards "$(pwd)")"; then
            config_dir_for_scan="$(dirname "$config_file_to_load")"
        else
            echo "❌ No vm.yaml found in current directory or parent directories" >&2
            echo "" >&2
            echo "To create a vm.yaml file for this project, run:" >&2
            echo "  vm init" >&2
            return 1
        fi
    elif [[ -n "$custom_config_path" ]]; then
        # Handle custom config path
        if [[ "$custom_config_path" = /* ]]; then
            config_file_to_load="$custom_config_path"
        else
            config_file_to_load="$(pwd)/$custom_config_path"
        fi

        # Handle directory path with vm.yaml
        if [[ ! -f "$config_file_to_load" && "$config_file_to_load" == */vm.yaml ]]; then
            local dir_path
            dir_path="$(dirname "$config_file_to_load")"
            if [[ -d "$dir_path" ]]; then
                echo "❌ No vm.yaml found in directory: $dir_path" >&2
                return 1
            fi
        fi

        if [[ ! -f "$config_file_to_load" ]]; then
            echo "❌ Custom config file not found: $config_file_to_load" >&2
            return 1
        fi
    else
        # Look for local vm.yaml
        if [[ -f "$local_config_path" ]]; then
            config_file_to_load="$local_config_path"
        else
            if [[ -t 0 && -t 1 ]]; then
                # TTY mode
                echo "❌ No vm.yaml configuration file found in $(pwd)" >&2
                echo "" >&2
                echo "To create a vm.yaml file for this project, run:" >&2
                echo "  vm init" >&2
                return 1
            else
                echo "❌ No vm.yaml configuration file found in $(pwd). Run \"vm init\" to create one." >&2
                return 1
            fi
        fi
    fi

    # Load and validate user config
    local user_config=""
    if [[ -f "$config_file_to_load" ]]; then
        

        # Validate file and load content
        # Use vm-config binary to validate
        if ! "$VM_CONFIG" validate "$config_file_to_load" >/dev/null 2>&1; then
                echo "❌ Invalid YAML in project config: $config_file_to_load" >&2
                return 1
            fi
            user_config="$(cat "$config_file_to_load")"
        else
            # Fallback: just read the file
            user_config="$(cat "$config_file_to_load")"
        fi

        # Check for valid top-level keys
        local valid_keys='["$schema","version","provider","project","vm","versions","ports","services","apt_packages","npm_packages","cargo_packages","pip_packages","aliases","environment","terminal","claude_sync","gemini_sync","persist_databases"]'
        local user_keys
        # Basic key validation using grep - check for at least one valid key
        local valid_keys_found=false
        for key in version provider project vm versions ports services apt_packages npm_packages cargo_packages pip_packages aliases environment terminal claude_sync gemini_sync persist_databases; do
            if grep -q "^${key}:" "$config_file_to_load"; then
                valid_keys_found=true
                break
            fi
        done

        if [[ "$valid_keys_found" == "false" ]] && [[ -s "$config_file_to_load" ]]; then
            echo "❌ Invalid configuration structure. No recognized configuration keys found." >&2
            return 1
        fi
    fi

    # Merge configurations
    local final_config
    if [[ -n "$user_config" ]]; then
        final_config="$(deep_merge_configs "$default_config" "$user_config")"
    else
        final_config="$default_config"
    fi

    echo "$final_config"
}

# Validate YAML configuration against YAML schema using Python
validate_against_yaml_schema() {
    local config="$1"
    local schema_path="$2"

    if [[ ! -f "$schema_path" ]]; then
        echo "Schema file not found: $schema_path"
        return 1
    fi

    # Check if Python dependencies are available
    local dependency_check
    if ! dependency_check=$(python3 -c "
try:
    import yaml
    import jsonschema
    print('dependencies_available')
except ImportError as e:
    print(f'missing_dependency: {e}')
" 2>&1); then
        echo "⚠️  Warning: Python not available for schema validation, using basic validation"
        echo "✅ Configuration format appears valid (basic validation only)"
        return 0
    fi

    if [[ "$dependency_check" =~ "missing_dependency" ]]; then
        echo "⚠️  Warning: Python dependencies missing for full schema validation: $(echo "$dependency_check" | sed 's/missing_dependency: //')"
        echo "✅ Configuration format appears valid (basic validation only)"
        return 0
    fi

    # Create temp file for config
    local temp_config="/tmp/vm-config-validate-$$.yaml"
    echo "$config" > "$temp_config"

    # Use Python to validate YAML against YAML schema
    local validation_output
    if validation_output=$(python3 -c "
import yaml
import jsonschema
import sys

try:
    # Load YAML schema
    with open('$schema_path', 'r') as f:
        schema = yaml.safe_load(f)

    # Load YAML config
    with open('$temp_config', 'r') as f:
        config = yaml.safe_load(f)

    # Validate
    jsonschema.validate(config, schema)
    print('✅ Configuration is valid')
except jsonschema.ValidationError as e:
    print(f'❌ Validation error: {e.message}')
    if e.path:
        print(f'   at path: {\".\".join(str(p) for p in e.path)}')
    sys.exit(1)
except Exception as e:
    print(f'❌ Error: {e}')
    sys.exit(1)
" 2>&1); then
        rm -f "$temp_config"
        return 0
    else
        rm -f "$temp_config"
        echo "$validation_output"
        return 1
    fi
}

# Validate merged configuration
validate_merged_config() {
    local config="$1"
    local errors=()
    local warnings=()

    # Basic validation that we have a YAML object with a project section
    local temp_config
    temp_config=$(mktemp)
    echo "$config" > "$temp_config"

    # Check for required project section using grep
    if ! grep -q '^project:' "$temp_config"; then
        errors+=("project section is required and must be an object")
        printf '%s\n' "${errors[@]}" >&2
        rm -f "$temp_config"
        return 1
    fi
    rm -f "$temp_config"

    # Schema-based validation using vm.schema.yaml
    local schema_path="$SCRIPT_DIR/vm.schema.yaml"
    if [[ -f "$schema_path" ]]; then
        local schema_errors
        if ! schema_errors=$(validate_against_yaml_schema "$config" "$schema_path"); then
            # Schema validation failed, add to errors
            errors+=("Schema validation failed: $schema_errors")
        fi
    else
        # Fallback to basic validation if schema not found
        local provider
        local temp_config
        temp_config=$(mktemp)
        echo "$config" > "$temp_config"
        # Query provider from config
        provider="$("$VM_CONFIG" query "$temp_config" 'provider' --raw --default 'docker' 2>/dev/null || echo 'docker')"
        else
            provider="$(grep '^provider:' "$temp_config" | awk '{print $2}' | head -1)"
            [[ -z "$provider" ]] && provider="docker"
        fi
        rm -f "$temp_config"
        if [[ "$provider" != "vagrant" && "$provider" != "docker" && "$provider" != "tart" ]]; then
            errors+=("provider must be 'vagrant', 'docker', or 'tart'")
        fi
    fi

    # Project validation
    local project_name
    local temp_config
    temp_config=$(mktemp)
    echo "$config" > "$temp_config"
    # Query project name from config
    project_name="$("$VM_CONFIG" query "$temp_config" 'project.name' --raw --default '' 2>/dev/null || echo '')"
    else
        project_name="$(grep -A 5 '^project:' "$temp_config" | grep 'name:' | awk '{print $2}' | head -1)"
    fi
    rm -f "$temp_config"
    if [[ -z "$project_name" ]]; then
        errors+=("project.name is required")
    elif ! echo "$project_name" | grep -qE '^[a-zA-Z0-9_-]+$'; then
        errors+=("project.name must contain only alphanumeric characters, hyphens, and underscores (got: '$project_name')")
    fi

    # Print errors and warnings
    if [[ ${#errors[@]} -gt 0 ]]; then
        echo "❌ Configuration validation failed:" >&2
        printf '  - %s\n' "${errors[@]}" >&2
        return 1
    fi

    if [[ ${#warnings[@]} -gt 0 ]]; then
        echo "⚠️  Configuration warnings:" >&2
        printf '  - %s\n' "${warnings[@]}" >&2
    fi

    return 0
}

# Main execution
main() {
    # Handle extract defaults command
    if [[ -n "$EXTRACT_DEFAULTS_PATH" ]]; then
        extract_schema_defaults "$EXTRACT_DEFAULTS_PATH"
        return $?
    fi

    # Handle init command
    if [[ "$INIT_FLAG" == "true" ]]; then
        initialize_vm_yaml "$CUSTOM_CONFIG_PATH"
        return $?
    fi

    # Load and merge config
    local final_config
    if ! final_config="$(load_and_merge_config "$CUSTOM_CONFIG_PATH")"; then
        return 1
    fi

    # Validate config
    if ! validate_merged_config "$final_config"; then
        return 1
    fi

    # Handle validation-only mode
    if [[ "$VALIDATE_FLAG" == "true" ]]; then
        echo "✅ Configuration is valid"
        return 0
    fi

    # Output final config (default behavior)
    # Add __config_dir field if we're in scan mode
    if [[ "$CUSTOM_CONFIG_PATH" == "__SCAN__" ]]; then
        # Find where the config was located by scanning again
        local config_location
        if config_location="$(find_vm_yaml_upwards "$(pwd)")"; then
            local config_dir
            config_dir="$(dirname "$config_location")"
            # Add config directory metadata to the final config
            final_config="${final_config}
__config_dir: $config_dir"
        fi
    fi

    echo "$final_config"
    return 0
}

# Debug output if VM_DEBUG is set
if [[ -n "${VM_DEBUG:-}" ]]; then
    echo "DEBUG validate-config.sh: CUSTOM_CONFIG_PATH='$CUSTOM_CONFIG_PATH'" >&2
    echo "DEBUG validate-config.sh: VALIDATE_FLAG='$VALIDATE_FLAG'" >&2
    echo "DEBUG validate-config.sh: GET_CONFIG_FLAG='$GET_CONFIG_FLAG'" >&2
    echo "DEBUG validate-config.sh: INIT_FLAG='$INIT_FLAG'" >&2
fi

# Run main function
main "$@"
