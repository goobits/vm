#!/bin/bash
# Shell wrapper for vm-config Rust binary
# This provides a command-line interface for the VM tool

set -e

# Get the VM tool directory dynamically
VM_TOOL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Path to the Rust binary
VM_CONFIG_BIN="${VM_CONFIG_BIN:-$VM_TOOL_DIR/rust/target/release/vm-config}"

# Check if binary exists - no external fallbacks
if [[ ! -x "$VM_CONFIG_BIN" ]]; then
    echo "Error: vm-config binary not found" >&2
    echo "Please build the vm-config binary:" >&2
    echo "  cd $VM_TOOL_DIR/rust/vm-config" >&2
    echo "  cargo build --release" >&2
    echo "" >&2
    echo "Or run the installer: ./install.sh" >&2
    exit 1
fi

# Parse command arguments and convert to vm-config commands
case "$1" in
    -r|--raw-output)
        # Raw output mode
        shift
        if [[ "$1" =~ ^\. ]]; then
            # Query mode: vm-config -r '.field' file
            field="${1#.}"
            shift
            "$VM_CONFIG_BIN" query "$1" "$field" --raw
        else
            # Complex queries not supported by wrapper
            echo "Error: Complex query not supported by vm-config wrapper" >&2
            echo "Raw output argument: $1" >&2
            exit 1
        fi
        ;;

    .)
        # Convert entire file: vm-config . file
        shift
        "$VM_CONFIG_BIN" convert "$1" -f yaml
        ;;

    -o|--output-format)
        # Output format: vm-config -o yaml file
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
        defaults="${1:-$VM_TOOL_DIR/vm.yaml}"
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
        # For simple queries like: vm-config '.project.name' file
        if [[ "$1" =~ ^\. ]]; then
            field="${1#.}"
            shift
            if [[ -f "$1" ]]; then
                "$VM_CONFIG_BIN" query "$1" "$field"
            else
                # No file specified, stdin not supported
                echo "Error: stdin input not supported by vm-config wrapper" >&2
                exit 1
            fi
        else
            # Unknown command, not supported
            echo "Error: Unknown command not supported by vm-config wrapper" >&2
            echo "Command: $1" >&2
            echo "Use vm-config directly for advanced operations" >&2
            exit 1
        fi
        ;;
esac