#!/bin/bash
# Vagrant wrapper script for Goobits
# Usage: ./packages/vm/vm.sh [command] [args...]

set -e

# Get the directory where this script is located (packages/vm)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Show usage information
show_usage() {
	echo "Usage: $0 [command] [args...]"
	echo ""
	echo "Commands:"
	echo "  validate              Validate VM configuration"
	echo "  up [args]            Start VM (vagrant up)"
	echo "  ssh [args]           SSH into VM (vagrant ssh)"
	echo "  halt [args]          Stop VM (vagrant halt)"
	echo "  destroy [args]       Destroy VM (vagrant destroy)"
	echo "  status [args]        Check VM status (vagrant status)"
	echo "  reload [args]        Reload VM (vagrant reload)"
	echo "  provision [args]     Reprovision VM (vagrant provision)"
	echo "  [vagrant-command]    Run any vagrant command"
	echo ""
	echo "Examples:"
	echo "  $0 validate          # Check configuration"
	echo "  $0 up                # Start the VM"
	echo "  $0 ssh               # Connect to VM"
	echo "  $0 halt              # Stop the VM"
}

# Change to the VM directory
cd "$SCRIPT_DIR"

# Handle special commands
case "${1:-}" in
	"validate")
		echo "🔍 Validating VM configuration..."
		node validate-config.mjs
		;;
	"help"|"-h"|"--help"|"")
		show_usage
		;;
	*)
		# Run vagrant with the project config
		VM_CONFIG="../../vm.json" vagrant "$@"
		;;
esac