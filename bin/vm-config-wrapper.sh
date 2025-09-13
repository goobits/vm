#!/bin/bash
# Shell wrapper for vm-config Rust binary
# This provides a yq-compatible interface for the VM tool

set -e

# Path to the Rust binary
VM_CONFIG_BIN="${VM_CONFIG_BIN:-/workspace/rust/vm-config/target/release/vm-config}"

# Check if binary exists, fall back to yq if not
if [[ ! -x "$VM_CONFIG_BIN" ]]; then
    # Fall back to yq if available
    if command -v yq >/dev/null 2>&1; then
        exec yq "$@"
    else
        echo "Error: vm-config binary not found and yq not available" >&2
        exit 1
    fi
fi

# Parse yq-style arguments and convert to vm-config commands
case "$1" in
    -r|--raw-output)
        # Raw output mode
        shift
        if [[ "$1" =~ ^\. ]]; then
            # Query mode: yq -r '.field' file
            field="${1#.}"
            shift
            "$VM_CONFIG_BIN" query "$1" "$field" --raw
        else
            # Pass through to yq for complex queries
            exec yq -r "$@"
        fi
        ;;

    .)
        # Convert entire file: yq . file
        shift
        "$VM_CONFIG_BIN" convert "$1" -f json
        ;;

    -o|--output-format)
        # Output format: yq -o yaml file
        format="$2"
        shift 2
        case "$format" in
            yaml|yml)
                "$VM_CONFIG_BIN" convert "$1" -f yaml
                ;;
            json)
                "$VM_CONFIG_BIN" convert "$1" -f json
                ;;
            *)
                echo "Unsupported format: $format" >&2
                exit 1
                ;;
        esac
        ;;

    validate)
        # Validate config: vm-config-wrapper validate file
        shift
        "$VM_CONFIG_BIN" validate "$1"
        ;;

    merge)
        # Merge configs: vm-config-wrapper merge base.yaml overlay.yaml
        shift
        base="$1"
        shift
        overlays=()
        for overlay in "$@"; do
            overlays+=("--overlay" "$overlay")
        done
        "$VM_CONFIG_BIN" merge --base "$base" "${overlays[@]}" -f yaml
        ;;

    process)
        # Full processing with defaults and presets
        shift
        defaults="${1:-/workspace/vm.yaml}"
        config="${2:-}"
        if [[ -n "$config" ]]; then
            "$VM_CONFIG_BIN" process --defaults "$defaults" --config "$config" -f yaml
        else
            "$VM_CONFIG_BIN" process --defaults "$defaults" -f yaml
        fi
        ;;

    detect-preset)
        # Detect preset for current directory
        shift
        dir="${1:-.}"
        "$VM_CONFIG_BIN" preset --dir "$dir" --detect-only
        ;;

    *)
        # For simple queries like: yq '.project.name' file
        if [[ "$1" =~ ^\. ]]; then
            field="${1#.}"
            shift
            if [[ -f "$1" ]]; then
                "$VM_CONFIG_BIN" query "$1" "$field"
            else
                # No file specified, try to use yq for stdin
                exec yq "$@"
            fi
        else
            # Unknown command, pass through to yq if available
            if command -v yq >/dev/null 2>&1; then
                exec yq "$@"
            else
                echo "Error: Unknown command and yq not available" >&2
                exit 1
            fi
        fi
        ;;
esac