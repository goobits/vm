#!/bin/bash
# Deep merge utility for VM configuration files
# Implements proper mixin semantics for configuration inheritance

set -e

# Function to perform deep merge with mixin semantics
deep_merge_configs() {
    local base_config="$1"
    local override_config="$2"
    
    # Use jq for the deep merge with special mixin handling
    echo "$base_config" | jq --argjson override "$override_config" '
        def is_empty_mixin(obj):
            if (obj | type) == "object" then
                (obj | length) == 0
            elif (obj | type) == "array" then
                (obj | length) == 0
            else
                false
            end;
            
        def deep_merge(base; override):
            if (base | type) == "object" and (override | type) == "object" then
                # Handle empty object as "inherit from base" mixin
                if is_empty_mixin(override) then
                    base
                else
                    # Deep merge objects, override values take precedence
                    base + reduce (override | keys_unsorted[]) as $key ({}; 
                        if base | has($key) then
                            .[$key] = deep_merge(base[$key]; override[$key])
                        else
                            .[$key] = override[$key]
                        end)
                end
            elif (base | type) == "array" and (override | type) == "array" then
                # Handle empty array as "inherit from base" mixin
                if is_empty_mixin(override) then
                    base
                else
                    # Merge arrays by concatenating unique values
                    (base + override) | unique
                end
            elif override == null then
                # null means "use base value"
                base
            else
                # For primitives, override wins
                override
            end;
        
        deep_merge(.; $override)
    '
}

# Function to merge a project config with default config
merge_project_config() {
    local default_config_path="$1"
    local project_config_path="$2"
    
    # Validate inputs
    if [[ ! -f "$default_config_path" ]]; then
        echo "❌ Default configuration not found: $default_config_path" >&2
        return 1
    fi
    
    if [[ ! -f "$project_config_path" ]]; then
        echo "❌ Project configuration not found: $project_config_path" >&2
        return 1
    fi
    
    # Load configurations (convert YAML to JSON for processing)
    local default_config
    if ! default_config="$(yq . "$default_config_path" 2>/dev/null)"; then
        echo "❌ Invalid YAML in default config: $default_config_path" >&2
        return 1
    fi
    
    local project_config
    if ! project_config="$(yq . "$project_config_path" 2>/dev/null)"; then
        echo "❌ Invalid YAML in project config: $project_config_path" >&2
        return 1
    fi
    
    # Perform deep merge
    deep_merge_configs "$default_config" "$project_config"
}

# Main function for command line usage
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