#!/bin/bash
# VM Configuration Processor V2 - Hybrid Shell/Rust Implementation
# Purpose: Thin wrapper around Rust config processor with shell orchestration
# This replaces the 823-line config-processor.sh with a streamlined version

set -e
set -u

# Get the directory where this script is located (works in both bash and zsh)
# Don't override SCRIPT_DIR if it's already set by the calling script
if [[ -z "${CONFIG_PROCESSOR_DIR:-}" ]]; then
    if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
        CONFIG_PROCESSOR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    elif [[ -n "${ZSH_VERSION:-}" ]]; then
        CONFIG_PROCESSOR_DIR="$(cd "$(dirname "${(%):-%x}")" && pwd)"
    else
        CONFIG_PROCESSOR_DIR="$(cd "$(dirname "$0")" && pwd)"
    fi
fi

# VM_TOOL_DIR is the parent of the shared directory
VM_TOOL_DIR="${VM_TOOL_DIR:-$(cd "$CONFIG_PROCESSOR_DIR/.." && pwd)}"

# Find the Rust config binary
# Priority order:
# 1. Environment variable VM_CONFIG_BIN
# 2. Compiled binary in the vm-tool directory
# 3. System-installed vm-config
# 4. Fall back to traditional yq
find_config_processor() {
    # Check environment variable first
    if [[ -n "${VM_CONFIG_BIN:-}" ]] && [[ -x "$VM_CONFIG_BIN" ]]; then
        echo "$VM_CONFIG_BIN"
        return 0
    fi

    # Check for compiled binary in vm-tool directory
    local rust_binary="$VM_TOOL_DIR/rust/vm-config/target/release/vm-config"
    if [[ -x "$rust_binary" ]]; then
        echo "$rust_binary"
        return 0
    fi

    # Check if vm-config is installed system-wide
    if command -v vm-config >/dev/null 2>&1; then
        echo "vm-config"
        return 0
    fi

    # No Rust implementation found
    return 1
}

# Initialize the config processor
VM_CONFIG=""
if VM_CONFIG="$(find_config_processor)"; then
    USE_RUST_CONFIG=true
else
    USE_RUST_CONFIG=false
    # Fall back to yq if available
    if ! command -v yq >/dev/null 2>&1; then
        echo "❌ Error: Neither vm-config nor yq is available" >&2
        echo "   Install the Rust config processor by running:" >&2
        echo "   cd $VM_TOOL_DIR/rust/vm-config && cargo build --release" >&2
        exit 1
    fi
fi

#=============================================================================
# MAIN CONFIG PROCESSING FUNCTIONS
#=============================================================================

# Process VM configuration with full merge logic
# Args: [config_path] [project_dir]
# Returns: Merged YAML configuration
process_vm_config() {
    local config_path="${1:-}"
    local project_dir="${2:-.}"
    local defaults_path="$VM_TOOL_DIR/vm.yaml"

    # Ensure defaults exist
    if [[ ! -f "$defaults_path" ]]; then
        echo "❌ Default configuration not found: $defaults_path" >&2
        return 1
    fi

    if [[ "$USE_RUST_CONFIG" == "true" ]]; then
        # Use Rust implementation
        local cmd=("$VM_CONFIG" "process")
        cmd+=("--defaults" "$defaults_path")

        if [[ -n "$config_path" ]] && [[ -f "$config_path" ]]; then
            cmd+=("--config" "$config_path")
        fi

        cmd+=("--project-dir" "$project_dir")
        cmd+=("--presets-dir" "$VM_TOOL_DIR/configs/presets")
        cmd+=("--format" "yaml")

        "${cmd[@]}"
    else
        # Fallback to shell/yq implementation
        # This is a simplified version - the full implementation would need more logic
        echo "⚠️  Using fallback yq implementation (limited functionality)" >&2

        if [[ -f "$config_path" ]]; then
            # Simple merge without preset detection
            yq eval-all 'select(fileIndex == 0) * select(fileIndex == 1)' \
                "$defaults_path" "$config_path"        else
            yq eval '.' "$defaults_path"        fi
    fi
}

# Validate configuration
# Args: config_path
# Returns: 0 if valid, 1 if invalid
validate_config() {
    local config_path="$1"

    if [[ ! -f "$config_path" ]]; then
        echo "❌ Configuration file not found: $config_path" >&2
        return 1
    fi

    if [[ "$USE_RUST_CONFIG" == "true" ]]; then
        "$VM_CONFIG" validate "$config_path"
    else
        # Basic validation with yq
        if yq eval '.' "$config_path" >/dev/null 2>&1; then
            echo "✅ Configuration syntax is valid"
            return 0
        else
            echo "❌ Invalid YAML syntax in $config_path" >&2
            return 1
        fi
    fi
}

# Query a specific field from configuration
# Args: config_path field_path
# Returns: Field value
query_config() {
    local config_path="$1"
    local field_path="$2"

    if [[ ! -f "$config_path" ]]; then
        echo ""
        return 1
    fi

    if [[ "$USE_RUST_CONFIG" == "true" ]]; then
        "$VM_CONFIG" query "$config_path" "$field_path" --raw 2>/dev/null || echo ""
    else
        # Fallback to yq
        yq eval "$field_path" "$config_path" 2>/dev/null || echo ""
    fi
}

# Merge multiple configurations
# Args: base_config overlay_config...
# Returns: Merged YAML
merge_configs() {
    local base="$1"
    shift

    if [[ "$USE_RUST_CONFIG" == "true" ]]; then
        local cmd=("$VM_CONFIG" "merge" "--base" "$base")
        for overlay in "$@"; do
            cmd+=("--overlay" "$overlay")
        done
        cmd+=("--format" "yaml")
        "${cmd[@]}"
    else
        # Fallback to yq merge
        local merge_cmd="yq eval-all"
        local merge_expr="select(fileIndex == 0)"
        local index=1

        for overlay in "$@"; do
            merge_expr+=" * select(fileIndex == $index)"
            ((index++))
        done

        $merge_cmd "$merge_expr" "$base" "$@"    fi
}

# Detect preset for project
# Args: project_dir
# Returns: Preset name or "base"
detect_preset() {
    local project_dir="${1:-.}"

    if [[ "$USE_RUST_CONFIG" == "true" ]]; then
        "$VM_CONFIG" preset \
            --dir "$project_dir" \
            --presets-dir "$VM_TOOL_DIR/configs/presets" \
            --detect-only
    else
        # Simplified preset detection
        if [[ -f "$project_dir/package.json" ]]; then
            if grep -q '"react"' "$project_dir/package.json" 2>/dev/null; then
                echo "react"
            else
                echo "nodejs"
            fi
        elif [[ -f "$project_dir/requirements.txt" ]] || [[ -f "$project_dir/setup.py" ]]; then
            echo "python"
        elif [[ -f "$project_dir/Cargo.toml" ]]; then
            echo "rust"
        elif [[ -f "$project_dir/go.mod" ]]; then
            echo "go"
        elif [[ -f "$project_dir/Gemfile" ]]; then
            echo "rails"
        elif [[ -f "$project_dir/manage.py" ]]; then
            echo "django"
        else
            echo "base"
        fi
    fi
}

#=============================================================================
# COMPATIBILITY FUNCTIONS (for backward compatibility)
#=============================================================================

# Legacy yq_raw function
yq_raw() {
    local filter="$1"
    local file="$2"
    query_config "$file" "$filter"
}

# Legacy yq_json function
yq_json() {
    local filter="$1"
    local file="$2"

    if [[ "$USE_RUST_CONFIG" == "true" ]]; then
        "$VM_CONFIG" query "$file" "$filter" 2>/dev/null || echo "null"
    else
        yq eval "$filter" "$file" 2>/dev/null || echo "null"
    fi
}

# Legacy validate_config_file function for vm.sh compatibility
validate_config_file() {
    local config_file="${1:-vm.yaml}"

    # Look for config file in current directory if not absolute path
    if [[ ! "$config_file" = /* ]] && [[ ! -f "$config_file" ]]; then
        if [[ -f "$(pwd)/$config_file" ]]; then
            config_file="$(pwd)/$config_file"
        fi
    fi

    validate_config "$config_file"
}

# Initialize a new config file
init_config_file() {
    local config_file="${1:-vm.yaml}"
    local project_dir="$(pwd)"
    local project_name="$(basename "$project_dir")"

    # Check if config already exists
    if [[ -f "$config_file" ]]; then
        echo "❌ Configuration already exists: $config_file" >&2
        echo "   Use --force to overwrite" >&2
        return 1
    fi

    # Create a basic config
    cat > "$config_file" <<EOF
# VM Configuration for $project_name
# Generated by vm init

project:
  name: $project_name
  hostname: ${project_name}.local

provider: docker  # or vagrant, tart

# Uncomment and modify as needed:
#
# ports:
#   web: 3000
#   api: 8080
#
# services:
#   postgresql:
#     enabled: true
#   redis:
#     enabled: true
#
# npm_packages:
#   - prettier
#   - eslint
EOF

    echo "✅ Created $config_file"
    echo ""
    echo "📝 Next steps:"
    echo "   1. Edit $config_file to customize your environment"
    echo "   2. Run 'vm create' to start your development environment"
    echo ""

    # Validate the created config
    validate_config "$config_file"
}

#=============================================================================
# INITIALIZATION MESSAGE
#=============================================================================

if [[ "${CONFIG_PROCESSOR_DEBUG:-}" == "true" ]]; then
    if [[ "$USE_RUST_CONFIG" == "true" ]]; then
        echo "🚀 Using Rust config processor: $VM_CONFIG" >&2
    else
        echo "⚠️  Using fallback yq implementation" >&2
    fi
fi

# Note: Function exporting works differently in bash vs zsh
# Scripts that source this file will have access to these functions directly