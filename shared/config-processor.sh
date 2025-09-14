#!/bin/bash
# Configuration processor for VM tool
# This script is a lightweight wrapper around the `vm-config` Rust binary,
# which now handles all configuration loading, merging, and validation.

# Get the directory where this script is located to reliably find the binary.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VM_CONFIG_BIN="$SCRIPT_DIR/../rust/target/release/vm-config"

# Check if the binary exists and is executable.
if [[ ! -x "$VM_CONFIG_BIN" ]]; then
    echo "Error: 'vm-config' binary not found or not executable at $VM_CONFIG_BIN" >&2
    echo "Please compile the Rust components by running: (cd rust && cargo build --release)" >&2
    exit 1
fi

# Pass all arguments directly to the vm-config binary.
"$VM_CONFIG_BIN" "$@"
