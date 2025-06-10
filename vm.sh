#!/bin/bash
# Vagrant wrapper script for Goobits
# Usage: ./packages/vm/vm.sh [command] [args...]

set -e

# Get the directory where this script is located (packages/vm)
# Handle both direct execution and npm link scenarios
if [ -L "$0" ]; then
	# If this is a symlink (npm link), resolve the real path
	REAL_SCRIPT="$(readlink -f "$0")"
	SCRIPT_DIR="$(cd "$(dirname "$REAL_SCRIPT")" && pwd)"
else
	# Direct execution
	SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Get the current working directory (where user ran the command)
CURRENT_DIR="$(pwd)"

# Function to find vm.json by searching upward from current directory
find_vm_json() {
	local dir="$1"
	local max_levels=3  # Search up to 3 levels
	local level=0
	
	while [ "$level" -lt "$max_levels" ] && [ "$dir" != "/" ]; do
		if [ -f "$dir/vm.json" ]; then
			echo "$dir/vm.json"
			return 0
		fi
		dir="$(dirname "$dir")"
		level=$((level + 1))
	done
	
	return 1
}

# Show usage information
show_usage() {
	echo "Usage: $0 [--config PATH] [command] [args...]"
	echo ""
	echo "Options:"
	echo "  --config PATH        Use specific vm.json file"
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
	echo "  kill                 Force kill VirtualBox processes (use when VMs are stuck)"
	echo "  [vagrant-command]    Run any vagrant command"
	echo ""
	echo "Examples:"
	echo "  $0 validate                    # Check configuration"
	echo "  $0 --config ./prod.json up     # Start VM with specific config"
	echo "  $0 up                          # Start the VM (auto-find vm.json)"
	echo "  $0 ssh                         # Connect to VM"
	echo "  $0 halt                        # Stop the VM"
	echo "  $0 kill                        # Kill stuck VirtualBox processes"
}

# Function to kill VirtualBox processes
kill_virtualbox() {
	echo "🔄 Terminating all VirtualBox processes..."
	
	# Clean up vagrant state first
	echo "🧹 Cleaning up Vagrant state..."
	if [ -d .vagrant ]; then
		rm -rf .vagrant
	fi
	
	# Force kill VirtualBox and ALL related processes
	echo "🔪 Force killing ALL VirtualBox processes..."
	pkill -9 -f "VBoxHeadless" || true
	pkill -9 -f "VBoxSVC" || true
	pkill -9 -f "VBoxXPCOMIPCD" || true
	pkill -9 -f "VirtualBox" || true
	
	echo "⏳ Waiting for VirtualBox services to terminate..."
	sleep 3
	
	echo "✅ All VirtualBox processes terminated!"
	echo ""
	echo "ℹ️ You may now need to manually clean up in the VirtualBox application"
	echo "ℹ️ or run 'vagrant up' to start your VM again."
}

# Parse --config flag
CUSTOM_CONFIG=""
if [ "$1" = "--config" ]; then
	if [ -z "$2" ]; then
		echo "❌ Error: --config requires a path argument"
		show_usage
		exit 1
	fi
	CUSTOM_CONFIG="$2"
	shift 2  # Remove --config and path from arguments
fi

# Change to the VM directory
cd "$SCRIPT_DIR"

# Handle special commands
case "${1:-}" in
	"validate")
		echo "🔍 Validating VM configuration..."
		if [ -n "$CUSTOM_CONFIG" ]; then
			# Convert relative path to absolute path
			if [[ "$CUSTOM_CONFIG" = /* ]]; then
				# Already absolute path
				FULL_CONFIG_PATH="$CUSTOM_CONFIG"
			else
				# Relative path, make it absolute from current directory
				FULL_CONFIG_PATH="$CURRENT_DIR/$CUSTOM_CONFIG"
			fi
			
			if [ -f "$FULL_CONFIG_PATH" ]; then
				echo "📍 Using custom config: $FULL_CONFIG_PATH"
				node "$SCRIPT_DIR/validate-config.js" "$FULL_CONFIG_PATH"
			else
				echo "❌ Error: Config file not found: $FULL_CONFIG_PATH"
				exit 1
			fi
		else
			# Search for vm.json in current directory and upward
			VM_JSON_PATH=$(find_vm_json "$CURRENT_DIR")
			if [ $? -eq 0 ]; then
				echo "📍 Found vm.json at: $VM_JSON_PATH"
				node "$SCRIPT_DIR/validate-config.js" "$VM_JSON_PATH"
			else
				echo "⚠️  No vm.json found, using defaults only"
				node "$SCRIPT_DIR/validate-config.js"
			fi
		fi
		;;
	"kill")
		kill_virtualbox
		;;
	"help"|"-h"|"--help"|"")
		show_usage
		;;
	*)
		if [ -n "$CUSTOM_CONFIG" ]; then
			# Convert relative path to absolute path
			if [[ "$CUSTOM_CONFIG" = /* ]]; then
				# Already absolute path
				FULL_CONFIG_PATH="$CUSTOM_CONFIG"
			else
				# Relative path, make it absolute from current directory
				FULL_CONFIG_PATH="$CURRENT_DIR/$CUSTOM_CONFIG"
			fi
			
			if [ -f "$FULL_CONFIG_PATH" ]; then
				echo "📍 Using custom config: $FULL_CONFIG_PATH"
				VM_PROJECT_DIR="$(dirname "$FULL_CONFIG_PATH")" VM_CONFIG="$FULL_CONFIG_PATH" vagrant "$@"
			else
				echo "❌ Error: Config file not found: $FULL_CONFIG_PATH"
				exit 1
			fi
		else
			# Search for vm.json in current directory and upward, then fall back to relative path
			VM_JSON_PATH=$(find_vm_json "$CURRENT_DIR")
			if [ $? -eq 0 ]; then
				echo "📍 Using vm.json from: $VM_JSON_PATH"
				VM_PROJECT_DIR="$(dirname "$VM_JSON_PATH")" VM_CONFIG="$VM_JSON_PATH" vagrant "$@"
			elif [ -f "../../vm.json" ]; then
				echo "📍 Using vm.json from: ../../vm.json"
				VM_CONFIG="../../vm.json" vagrant "$@"
			else
				echo "⚠️  No vm.json found, using defaults only"
				vagrant "$@"
			fi
		fi
		;;
esac