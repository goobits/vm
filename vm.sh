#!/bin/bash
# Vagrant wrapper script for sketchpad-com-next
# Usage: ./packages/vm/vm.sh [vagrant-command] [args...]

set -e

# Get the directory where this script is located (packages/vm)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Change to the VM directory and run vagrant with the project config
cd "$SCRIPT_DIR"
VM_CONFIG="../../vm.json" vagrant "$@"