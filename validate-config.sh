#!/bin/bash
# VM Configuration Manager - Shell version
# Purpose: Load, merge, validate, and output final configuration using jq
# Usage: ./validate-config.sh [--validate] [--get-config] [--init] [config-path]

set -e

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Initialize variables
VALIDATE_FLAG=""
GET_CONFIG_FLAG=""
INIT_FLAG=""
CUSTOM_CONFIG_PATH=""

# Parse arguments
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

# Deep merge function using jq
deep_merge() {
    local base_config="$1"
    local override_config="$2"
    
    echo "$base_config" | jq --argjson override "$override_config" '
        def deepmerge(a; b):
            if (a | type) == "object" and (b | type) == "object" then
                reduce (b | keys_unsorted[]) as $key (a; 
                    .[$key] = deepmerge(.[$key]; b[$key]))
            elif b == null then a
            else b end;
        deepmerge(.; $override)
    '
}

# Extract default values from schema
extract_schema_defaults() {
    local schema_path="$1"
    
    if [[ ! -f "$schema_path" ]]; then
        echo "❌ Schema file not found: $schema_path" >&2
        return 1
    fi
    
    jq '
    {
      provider: .properties.provider.default,
      project: {
        workspace_path: .properties.project.properties.workspace_path.default,
        backup_pattern: .properties.project.properties.backup_pattern.default
      },
      vm: {
        box: .properties.vm.properties.box.default,
        memory: .properties.vm.properties.memory.default,
        cpus: .properties.vm.properties.cpus.default,
        user: .properties.vm.properties.user.default,
        port_binding: .properties.vm.properties.port_binding.default,
        timezone: .properties.vm.properties.timezone.default
      },
      versions: {
        node: .properties.versions.properties.node.default,
        nvm: .properties.versions.properties.nvm.default,
        pnpm: .properties.versions.properties.pnpm.default
      },
      terminal: {
        emoji: .properties.terminal.properties.emoji.default,
        username: .properties.terminal.properties.username.default,
        theme: .properties.terminal.properties.theme.default,
        show_git_branch: .properties.terminal.properties.show_git_branch.default
      },
      aliases: .properties.aliases.default,
      claude_sync: .properties.claude_sync.default,
      gemini_sync: .properties.gemini_sync.default
    }' "$schema_path"
}

# Find vm.json upwards from directory
find_vm_json_upwards() {
    local start_dir="$1"
    local current_dir="$(cd "$start_dir" && pwd)"
    
    while [[ "$current_dir" != "/" ]]; do
        if [[ -f "$current_dir/vm.json" ]]; then
            echo "$current_dir/vm.json"
            return 0
        fi
        current_dir="$(dirname "$current_dir")"
    done
    
    # Check root directory
    if [[ -f "/vm.json" ]]; then
        echo "/vm.json"
        return 0
    fi
    
    return 1
}

# Initialize vm.json
initialize_vm_json() {
    local target_path="$1"
    local local_config_path="${target_path:-$(pwd)/vm.json}"
    
    # Check if vm.json already exists
    if [[ -f "$local_config_path" ]]; then
        echo "❌ vm.json already exists at $local_config_path" >&2
        echo "Use --config to specify a different location or remove the existing file." >&2
        return 1
    fi
    
    # Load default configuration from schema
    local schema_path="$SCRIPT_DIR/vm.schema.json"
    if [[ ! -f "$schema_path" ]]; then
        echo "❌ Schema file not found at $schema_path" >&2
        return 1
    fi
    
    local default_config="$(extract_schema_defaults "$schema_path")"
    local dir_name="$(basename "$(pwd)")"
    
    # Customize config for this directory
    local customized_config="$(echo "$default_config" | jq --arg dirname "$dir_name" '
        .project.name = $dirname |
        .project.hostname = "dev." + $dirname + ".local" |
        .terminal.username = $dirname + "-dev"
    ')"
    
    # Write the customized config
    if echo "$customized_config" | jq . > "$local_config_path"; then
        echo "✅ Created vm.json for project: $dir_name"
        echo "📍 Configuration file: $local_config_path"
        echo ""
        echo "Next steps:"
        echo "  1. Review and customize vm.json as needed"
        echo "  2. Run \"vm create\" to start your development environment"
        return 0
    else
        echo "❌ Failed to create vm.json" >&2
        return 1
    fi
}

# Load and merge configuration
load_and_merge_config() {
    local custom_config_path="$1"
    local local_config_path="$(pwd)/vm.json"
    local config_file_to_load=""
    local config_dir_for_scan=""
    
    # Load default config from schema
    local schema_path="$SCRIPT_DIR/vm.schema.json"
    if [[ ! -f "$schema_path" ]]; then
        echo "❌ Schema file not found at $schema_path" >&2
        return 1
    fi
    
    local default_config="$(extract_schema_defaults "$schema_path")"
    
    # Determine which config to load
    if [[ "$custom_config_path" == "__SCAN__" ]]; then
        # Scan upwards for vm.json
        if config_file_to_load="$(find_vm_json_upwards "$(pwd)")"; then
            config_dir_for_scan="$(dirname "$config_file_to_load")"
        else
            echo "❌ No vm.json found in current directory or parent directories" >&2
            echo "" >&2
            echo "To create a vm.json file for this project, run:" >&2
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
        
        # Handle directory path with vm.json
        if [[ ! -f "$config_file_to_load" && "$config_file_to_load" == */vm.json ]]; then
            local dir_path="$(dirname "$config_file_to_load")"
            if [[ -d "$dir_path" ]]; then
                echo "❌ No vm.json found in directory: $dir_path" >&2
                return 1
            fi
        fi
        
        if [[ ! -f "$config_file_to_load" ]]; then
            echo "❌ Custom config file not found: $config_file_to_load" >&2
            return 1
        fi
    else
        # Look for local vm.json
        if [[ -f "$local_config_path" ]]; then
            config_file_to_load="$local_config_path"
        else
            if [[ -t 0 && -t 1 ]]; then
                # TTY mode
                echo "❌ No vm.json configuration file found in $(pwd)" >&2
                echo "" >&2
                echo "To create a vm.json file for this project, run:" >&2
                echo "  vm init" >&2
                return 1
            else
                echo "❌ No vm.json configuration file found in $(pwd). Run \"vm init\" to create one." >&2
                return 1
            fi
        fi
    fi
    
    # Load and validate user config
    local user_config=""
    if [[ -f "$config_file_to_load" ]]; then
        if ! user_config="$(jq . "$config_file_to_load" 2>/dev/null)"; then
            echo "❌ Invalid JSON in project config: $config_file_to_load" >&2
            return 1
        fi
        
        # Check for valid top-level keys
        local valid_keys='["$schema","provider","project","vm","versions","ports","services","apt_packages","npm_packages","cargo_packages","pip_packages","aliases","environment","terminal","claude_sync","gemini_sync","persist_databases"]'
        local user_keys="$(echo "$user_config" | jq -r 'keys[]')"
        local has_valid_keys="$(echo "$user_config" | jq --argjson valid "$valid_keys" 'keys as $uk | $valid as $vk | ($uk | map(. as $k | $vk | contains([$k])) | any)')"
        
        if [[ "$has_valid_keys" == "false" && -n "$user_keys" ]]; then
            local user_keys_str="$(echo "$user_config" | jq -r 'keys | join(", ")')"
            echo "❌ Invalid configuration structure. No recognized configuration keys found. Got: $user_keys_str" >&2
            return 1
        fi
    fi
    
    # Merge configurations
    local final_config
    if [[ -n "$user_config" ]]; then
        final_config="$(deep_merge "$default_config" "$user_config")"
    else
        final_config="$default_config"
    fi
    
    # Add metadata for scan mode
    if [[ -n "$config_dir_for_scan" ]]; then
        final_config="$(echo "$final_config" | jq --arg dir "$config_dir_for_scan" '. + {"__config_dir": $dir}')"
    fi
    
    echo "$final_config"
}

# Validate merged configuration
validate_merged_config() {
    local config="$1"
    local errors=()
    local warnings=()
    
    # Check if config is valid object
    if ! echo "$config" | jq -e 'type == "object"' >/dev/null 2>&1; then
        errors+=("Configuration must be a valid JSON object")
        printf '%s\n' "${errors[@]}" >&2
        return 1
    fi
    
    # Check for required project section
    if ! echo "$config" | jq -e '.project | type == "object"' >/dev/null 2>&1; then
        errors+=("project section is required and must be an object")
        printf '%s\n' "${errors[@]}" >&2
        return 1
    fi
    
    # Provider validation
    local provider="$(echo "$config" | jq -r '.provider // "docker"')"
    if [[ "$provider" != "vagrant" && "$provider" != "docker" ]]; then
        errors+=("provider must be 'vagrant' or 'docker'")
    fi
    
    # Project validation
    local project_name="$(echo "$config" | jq -r '.project.name // ""')"
    if [[ -z "$project_name" ]]; then
        errors+=("project.name is required")
    elif ! echo "$project_name" | grep -qE '^[a-zA-Z0-9_-]+$'; then
        errors+=("project.name must contain only alphanumeric characters, hyphens, and underscores")
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
    # Handle init command
    if [[ "$INIT_FLAG" == "true" ]]; then
        initialize_vm_json "$CUSTOM_CONFIG_PATH"
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
        if config_location="$(find_vm_json_upwards "$(pwd)")"; then
            local config_dir="$(dirname "$config_location")"
            final_config="$(echo "$final_config" | jq --arg dir "$config_dir" '. + {__config_dir: $dir}')"
        fi
    fi
    
    echo "$final_config"
    return 0
}

# Debug output if VM_DEBUG is set
if [[ -n "$VM_DEBUG" ]]; then
    echo "DEBUG validate-config.sh: CUSTOM_CONFIG_PATH='$CUSTOM_CONFIG_PATH'" >&2
    echo "DEBUG validate-config.sh: VALIDATE_FLAG='$VALIDATE_FLAG'" >&2
    echo "DEBUG validate-config.sh: GET_CONFIG_FLAG='$GET_CONFIG_FLAG'" >&2
    echo "DEBUG validate-config.sh: INIT_FLAG='$INIT_FLAG'" >&2
fi

# Run main function
main "$@"