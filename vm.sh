#!/bin/bash
# VM wrapper script for Goobits - supports both Vagrant and Docker
# Usage: ./packages/vm/vm.sh [command] [args...]

set -e

# Check for required tools
if ! command -v yq &> /dev/null; then
    echo "❌ Error: yq is not installed. This tool is required for YAML processing."
    echo ""
    echo "📦 To install yq on Ubuntu/Debian:"
    echo "   sudo apt-get update"
    echo "   sudo apt-get install yq"
    echo ""
    echo "📦 To install yq on macOS:"
    echo "   brew install yq"
    echo ""
    echo "📦 To install yq on other systems:"
    echo "   Visit: https://github.com/kislyuk/yq"
    echo ""
    exit 1
fi

# Default port configuration
DEFAULT_POSTGRES_PORT=5432
DEFAULT_REDIS_PORT=6379
DEFAULT_MONGODB_PORT=27017

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

# Parse comma-separated mount string into mount arguments
# Note: Directory names containing commas are not supported due to parsing complexity
parse_mount_string() {
	local mount_str="$1"
	local mount_args=""
	
	if [ -n "$mount_str" ]; then
		# Split by comma and process each mount (save original IFS)
		local old_ifs="$IFS"
		IFS=','
		local MOUNTS=($mount_str)  # This is intentionally unquoted for word splitting
		IFS="$old_ifs"
		
		# Pre-validate: Detect obvious comma-in-name issues  
		# Check if any parsed fragment looks suspicious (very short, no path separators)
		# and if so, warn about comma limitation
		local suspicious_count=0
		local total_count=${#MOUNTS[@]}
		for test_mount in "${MOUNTS[@]}"; do
			test_mount=$(echo "$test_mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
			# Remove permission suffix for testing
			if [[ "$test_mount" == *:* ]]; then
				test_mount="${test_mount%:*}"
			fi
			# Very short fragments (1-2 chars) without slashes are suspicious  
			if [[ -n "$test_mount" ]] && [[ ${#test_mount} -le 2 ]] && [[ "$test_mount" != *"/"* ]] && [[ "$test_mount" != "."* ]]; then
				((suspicious_count++))
			fi
		done
		
		# If more than half the fragments are suspicious short names, likely comma issue
		if [[ $total_count -gt 2 ]] && [[ $suspicious_count -gt $((total_count / 2)) ]]; then
			echo "❌ Error: Possible comma-containing directory names detected" >&2
			echo "   Parsed fragments: ${MOUNTS[*]}" >&2  
			echo "   Directory names containing commas are not supported" >&2
			echo "   Tip: Use symlinks like: ln -s 'dir,with,commas' dir-without-commas" >&2
			return 1
		fi
		for mount in "${MOUNTS[@]}"; do
			# Trim whitespace
			mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
			
			# Handle mount:permission format (e.g., ./src:rw, ./config:ro)
			if [[ "$mount" == *:* ]]; then
				local source="${mount%:*}"
				local perm="${mount##*:}"
				# Check if source exists and is a directory
				if [ ! -d "$source" ]; then
					echo "❌ Error: Directory '$source' does not exist or is not a directory" >&2
					return 1
				fi
				case "$perm" in
					"ro"|"readonly")
						mount_args="$mount_args -v $(realpath "$source"):/workspace/$(basename "$source"):ro"
						;;
					"rw"|"readwrite"|*)
						mount_args="$mount_args -v $(realpath "$source"):/workspace/$(basename "$source")"
						;;
				esac
			else
				# Check if mount exists and is a directory
				if [ ! -d "$mount" ]; then
					echo "❌ Error: Directory '$mount' does not exist or is not a directory" >&2
					return 1
				fi
				# Default to read-write mount
				mount_args="$mount_args -v $(realpath "$mount"):/workspace/$(basename "$mount")"
			fi
		done
	fi
	
	echo "$mount_args"
}

# Docker wrapper to handle sudo requirements
docker_cmd() {
	if ! docker version &>/dev/null 2>&1; then
		sudo docker "$@"
	else
		docker "$@"
	fi
}

# Docker compose wrapper to handle both docker-compose and docker compose
docker_compose() {
	# Check if we need sudo for docker
	local docker_prefix=""
	if ! docker version &>/dev/null 2>&1; then
		docker_prefix="sudo"
	fi
	
	if command -v docker-compose &> /dev/null; then
		$docker_prefix docker-compose "$@"
	else
		$docker_prefix docker compose "$@"
	fi
}


# Show usage information
show_usage() {
	echo "Usage: $0 [--config [PATH]] [--debug] [--dry-run] [--auto-login [true|false]] [command] [args...]"
	echo ""
	echo "Options:"
	echo "  --config [PATH]      Use specific vm.yaml file, or scan up directory tree if no path given"
	echo "  --debug              Enable debug output"
	echo "  --dry-run            Show what would be executed without actually running it"
	echo "  --auto-login [BOOL]  Automatically SSH into VM after create/start (default: true)"
	echo ""
	echo "Commands:"
	echo "  init                  Initialize a new vm.yaml configuration file"
	echo "  generate              Generate vm.yaml by composing services"
	echo "  validate              Validate VM configuration"
	echo "  list                  List all VM instances"
	echo "  temp [mounts] [--auto-destroy]  Create temporary VM with specific directory mounts"
	echo "  temp destroy                    Destroy the temporary VM"
	echo "  create [args]         Create new VM with full provisioning"
	echo "  start [args]          Start existing VM without provisioning"
	echo "  stop [args]           Stop VM but keep data"
	echo "  restart [args]        Restart VM without reprovisioning"
	echo "  ssh [args]            SSH into VM"
	echo "  destroy [args]        Destroy VM completely"
	echo "  status [args]         Check VM status"
	echo "  provision [args]      Re-run full provisioning on existing VM"
	echo "  logs [args]           View VM logs (Docker only)"
	echo "  exec [args]           Execute command in VM (Docker only)"
	echo "  test [args]           Run VM test suite"
	echo "  kill                  Force kill VM processes"
	echo ""
	echo "Examples:"
	echo "  vm generate --services postgresql,redis  # Generate config with services"
	echo "  vm generate --ports 3020 --name my-app   # Generate with custom ports/name"
	echo "  vm validate                              # Check configuration"
	echo "  vm list                                  # List all VM instances"
	echo "  vm temp ./client,./server,./shared       # Create temp VM with specific folders"
	echo "  vm temp ./src:rw,./config:ro             # Temp VM with mount permissions"
	echo "  vm temp ./src --auto-destroy             # Temp VM that destroys on exit"
	echo "  vm temp destroy                          # Destroy temp VM"
	echo "  vm --config ./prod.yaml create           # Create VM with specific config"
	echo "  vm --config create                       # Create VM scanning up for vm.yaml"
	echo "  vm create                                # Create new VM (auto-find vm.yaml)"
	echo "  vm create --auto-login=false             # Create VM without auto SSH"
	echo "  vm start                                 # Start existing VM (fast)"
	echo "  vm start --auto-login=false              # Start VM without auto SSH"
	echo "  vm ssh                                   # Connect to VM"
	echo "  vm stop                                  # Stop the VM"
	echo ""
	echo "The provider (Vagrant or Docker) is determined by the 'provider' field in vm.yaml"
}

# Function to kill VirtualBox processes
kill_virtualbox() {
	echo "🔄 Terminating all VirtualBox processes..."
	
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

# Function to load and validate config (delegated to validate-config.sh)
load_config() {
	local config_path="$1"
	local original_dir="$2"
	
	# Debug output if --debug flag is set
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG load_config: config_path='$config_path', original_dir='$original_dir'" >&2
		echo "DEBUG load_config: SCRIPT_DIR='$SCRIPT_DIR'" >&2
	fi
	
	if [ -n "$config_path" ]; then
		# Use custom config path
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG load_config: Running: cd '$original_dir' && '$SCRIPT_DIR/validate-config.sh' --get-config '$config_path'" >&2
		fi
		(cd "$original_dir" && "$SCRIPT_DIR/validate-config.sh" --get-config "$config_path")
	else
		# Use default discovery logic - run from the original directory
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG load_config: Running: cd '$original_dir' && '$SCRIPT_DIR/validate-config.sh' --get-config" >&2
		fi
		(cd "$original_dir" && "$SCRIPT_DIR/validate-config.sh" --get-config)
	fi
}

# Get provider from config
get_provider() {
	local config="$1"
	echo "$config" | yq -r '.provider // "docker"'
}

# Docker helper function to reduce duplication
docker_run() {
	local action="$1"
	local config="$2"
	local project_dir="$3"
	shift 3
	
	# Extract project name once
	local project_name=$(echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]')
	local container_name="${project_name}-dev"
	
	case "$action" in
		"compose")
			cd "$project_dir"
			docker_compose "$@"
			;;
		"exec")
			docker_cmd exec "${container_name}" "$@"
			;;
		"exec-it")
			docker_cmd exec -it "${container_name}" "$@"
			;;
		*)
			cd "$project_dir"
			docker_compose "$action" "$@"
			;;
	esac
}

# Docker functions
docker_up() {
	local config="$1"
	local project_dir="$2"
	local auto_login="$3"
	shift 3
	
	echo "🚀 Starting development environment..."
	
	# Create a secure temporary file for the config
	TEMP_CONFIG_FILE=$(mktemp /tmp/vm-config.XXXXXX)
	# Ensure the temp file is removed when the script exits
	trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT
	
	# Generate docker-compose.yml
	echo "$config" > "$TEMP_CONFIG_FILE"
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"
	
	# Build and start containers
	docker_run "compose" "$config" "$project_dir" build
	docker_run "compose" "$config" "$project_dir" up -d "$@"
	
	# Get container name
	local project_name=$(echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]')
	local container_name="${project_name}-dev"
	
	# Wait for container to be ready before proceeding
	echo "⏳ Initializing container..."
	local max_attempts=30
	local attempt=1
	while [ $attempt -le $max_attempts ]; do
		# Use docker_cmd to handle sudo if needed, and check container is running
		if docker_cmd inspect "${container_name}" --format='{{.State.Status}}' 2>/dev/null | grep -q "running"; then
			# Also verify we can exec into it
			if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
				echo "✅ Container is ready"
				break
			fi
		fi
		if [ $attempt -eq $max_attempts ]; then
			echo "❌ Environment initialization failed"
			return 1
		fi
		echo "⏳ Starting up... ($attempt/$max_attempts)"
		sleep 2
		((attempt++))
	done
	
	# Copy config file to container
	echo "📋 Loading project configuration..."
	if docker_cmd cp "$TEMP_CONFIG_FILE" "${container_name}:/tmp/vm-config.json"; then
		echo "✅ Configuration loaded"
	else
		echo "❌ Configuration loading failed"
		return 1
	fi
	
	# Fix volume permissions before Ansible
	echo "🔑 Setting up permissions..."
	local project_user=$(echo "$config" | yq -r '.vm.user // "developer"')
	if docker_run "exec" "$config" "$project_dir" chown -R "$project_user:$project_user" "/home/$project_user/.nvm" "/home/$project_user/.cache"; then
		echo "✅ Permissions configured"
	else
		echo "⚠️ Permission setup skipped (non-critical)"
	fi
	
	# VM tool directory is already mounted read-only via docker-compose
	
	# Run Ansible playbook inside the container
	echo "🔧 Provisioning development environment..."
	
	# Check if debug mode is enabled
	ANSIBLE_VERBOSITY=""
	ANSIBLE_DIFF=""
	if [ "${VM_DEBUG:-}" = "true" ] || [ "${DEBUG:-}" = "true" ]; then
		echo "🐛 Debug mode enabled - showing detailed Ansible output"
		ANSIBLE_VERBOSITY="-vvv"
		ANSIBLE_DIFF="--diff"
	fi
	
	# Create log file path
	ANSIBLE_LOG="/tmp/ansible-provision-$(date +%Y%m%d-%H%M%S).log"
	
	if docker_run "exec" "$config" "$project_dir" bash -c "ansible-playbook \
		-i localhost, \
		-c local \
		$ANSIBLE_VERBOSITY \
		$ANSIBLE_DIFF \
		/vm-tool/shared/ansible/playbook.yml 2>&1 | tee $ANSIBLE_LOG"; then
		echo "🎉 Development environment ready!"
	else
		ANSIBLE_EXIT_CODE=$?
		echo "⚠️ Provisioning completed with warnings (exit code: $ANSIBLE_EXIT_CODE)"
		echo "📋 Full log saved in container at: $ANSIBLE_LOG"
		echo "💡 Tips:"
		echo "   - Run with VM_DEBUG=true vm create to see detailed error output"
		echo "   - View the log: vm exec cat $ANSIBLE_LOG"
		echo "   - Or copy it: docker cp ${container_name}:$ANSIBLE_LOG ./ansible-error.log"
	fi
	
	# Ensure supervisor services are started
	echo "🚀 Starting services..."
	docker_run "exec" "$config" "$project_dir" bash -c "supervisorctl reread && supervisorctl update" || true
	
	# Clean up generated docker-compose.yml since containers are now running
	local compose_file="${project_dir}/docker-compose.yml"
	if [ -f "$compose_file" ]; then
		echo "✨ Cleanup complete"
		rm "$compose_file"
	fi
	
	echo "🎉 Environment ready!"
	
	# Automatically SSH into the container if auto-login is enabled
	if [ "$auto_login" = "true" ]; then
		echo "🌟 Entering development environment..."
		docker_ssh "$config" "" "."
	else
		echo "💡 Use 'vm ssh' to connect to the environment"
	fi
}

docker_ssh() {
	local config="$1"
	local project_dir="$2"
	local relative_path="$3"
	shift 3
	
	# Get workspace path and user from config
	local workspace_path=$(echo "$config" | yq -r '.project.workspace_path // "/workspace"')
	local project_user=$(echo "$config" | yq -r '.vm.user // "developer"')
	local target_dir="${workspace_path}"
	
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG docker_ssh: relative_path='$relative_path'" >&2
		echo "DEBUG docker_ssh: workspace_path='$workspace_path'" >&2
	fi
	
	# If we have a relative path and it's not just ".", append it to workspace path
	if [ -n "$relative_path" ] && [ "$relative_path" != "." ]; then
		target_dir="${workspace_path}/${relative_path}"
	fi
	
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG docker_ssh: target_dir='$target_dir'" >&2
	fi
	
	# Handle -c flag specifically for command execution
	if [ "$1" = "-c" ] && [ -n "$2" ]; then
		# Run command non-interactively
		docker_run "exec" "$config" "" su - $project_user -c "cd '$target_dir' && source ~/.zshrc && $2"
	elif [ $# -gt 0 ]; then
		# Run with all arguments
		docker_run "exec" "$config" "" su - $project_user -c "cd '$target_dir' && source ~/.zshrc && zsh $*"
	else
		# Interactive mode - use a simple approach that works
		local project_name=$(echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]')
		local container_name="${project_name}-dev"
		
		# Run an interactive shell that properly handles signals
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG docker_ssh: Executing: docker exec -it -e VM_TARGET_DIR='$target_dir' ${container_name} sudo -u $project_user bash -c \"cd '$target_dir' && exec /bin/zsh\"" >&2
		fi
		# Set environment variable and use bash to ensure directory change
		docker_cmd exec -it -e "VM_TARGET_DIR=$target_dir" "${container_name}" sudo -u "$project_user" bash -c "
			cd '$target_dir' || exit 1
			exec /bin/zsh
		"
	fi
}

docker_start() {
	local config="$1"
	local project_dir="$2"
	local relative_path="$3"
	local auto_login="$4"
	shift 4
	
	echo "🚀 Starting development environment..."
	
	# Get container name
	local project_name=$(echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]')
	local container_name="${project_name}-dev"
	
	# Check if container exists
	if ! docker_cmd inspect "${container_name}" >/dev/null 2>&1; then
		echo "❌ Container doesn't exist. Use 'vm create' to set up the environment first."
		return 1
	fi
	
	# Start the container directly (not using docker-compose)
	docker_cmd start "${container_name}" "$@"
	
	# Wait for container to be ready
	echo "⏳ Starting up..."
	local max_attempts=15
	local attempt=1
	while [ $attempt -le $max_attempts ]; do
		if docker_cmd exec "${container_name}" echo "ready" >/dev/null 2>&1; then
			echo "✅ Environment ready!"
			break
		fi
		if [ $attempt -eq $max_attempts ]; then
			echo "❌ Environment startup failed"
			return 1
		fi
		sleep 1
		((attempt++))
	done
	
	echo "🎉 Environment started!"
	
	# Automatically SSH into the container if auto-login is enabled
	if [ "$auto_login" = "true" ]; then
		echo "🌟 Entering development environment..."
		docker_ssh "$config" "$project_dir" "$relative_path"
	else
		echo "💡 Use 'vm ssh' to connect to the environment"
	fi
}

docker_halt() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	# Stop the container directly (not using docker-compose)
	local project_name=$(echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]')
	local container_name="${project_name}-dev"
	docker_cmd stop "${container_name}" "$@"
}

docker_destroy() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	# Get project name for user feedback
	local project_name=$(echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]')
	local container_name="${project_name}-dev"
	
	echo "🗑️ Destroying VM: ${container_name}"
	
	# Create a secure temporary file for the config
	TEMP_CONFIG_FILE=$(mktemp /tmp/vm-config.XXXXXX)
	# Ensure the temp file is removed when the script exits
	trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT
	
	# Generate docker-compose.yml temporarily for destroy operation
	echo "🧹 Preparing cleanup..."
	echo "$config" > "$TEMP_CONFIG_FILE"
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"
	
	# Run docker compose down with volumes
	docker_run "down" "$config" "$project_dir" -v "$@"
	
	# Clean up the generated docker-compose.yml after destroy
	local compose_file="${project_dir}/docker-compose.yml"
	if [ -f "$compose_file" ]; then
		echo "✨ Cleanup complete"
		rm "$compose_file"
	fi
}

docker_status() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	docker_run "ps" "$config" "$project_dir" "$@"
}

docker_reload() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	docker_halt "$config" "$project_dir"
	docker_start "$config" "$project_dir" "$@"
}

docker_provision() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	echo "🔄 Rebuilding environment..."
	
	# Create a secure temporary file for the config
	TEMP_CONFIG_FILE=$(mktemp /tmp/vm-config.XXXXXX)
	# Ensure the temp file is removed when the script exits
	trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT
	
	# Generate fresh docker-compose.yml for provisioning
	echo "$config" > "$TEMP_CONFIG_FILE"
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$TEMP_CONFIG_FILE" "$project_dir"
	
	docker_run "compose" "$config" "$project_dir" build --no-cache
	docker_run "compose" "$config" "$project_dir" up -d "$@"
	
	# Clean up generated docker-compose.yml since containers are now running
	local compose_file="${project_dir}/docker-compose.yml"
	if [ -f "$compose_file" ]; then
		echo "✨ Cleanup complete"
		rm "$compose_file"
	fi
}

docker_logs() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	docker_run "logs" "$config" "$project_dir" "$@"
}

docker_exec() {
	local config="$1"
	shift
	
	docker_run "exec" "$config" "" "$@"
}

docker_kill() {
	echo "⏹️ Stopping environment..."
	local config="$1"
	local project_name=$(echo "$config" | yq -r '.project.name' | tr -cd '[:alnum:]')
	
	docker_cmd stop "${project_name}-dev" 2>/dev/null || true
	docker_cmd stop "${project_name}-postgres" 2>/dev/null || true
	docker_cmd stop "${project_name}-redis" 2>/dev/null || true
	docker_cmd stop "${project_name}-mongodb" 2>/dev/null || true
	
	echo "✅ All Docker containers stopped!"
}

# List all VM instances
vm_list() {
	echo "📋 VM Instances:"
	echo "=================="
	
	# Check if Docker is available
	if command -v docker &> /dev/null; then
		echo ""
		echo "🐳 Docker VMs:"
		echo "--------------"
		
		# Get all containers and filter for VM-like names
		local vm_containers=$(docker_cmd ps -a --format "{{.Names}}\t{{.Status}}\t{{.CreatedAt}}" | awk '$1 ~ /-dev$/ || $1 ~ /postgres/ || $1 ~ /redis/ || $1 ~ /mongodb/ {print}' 2>/dev/null || true)
		
		if [ -n "$vm_containers" ]; then
			echo "NAME                    STATUS                       CREATED"
			echo "================================================================"
			echo "$vm_containers" | while IFS=$'\t' read -r name status created; do
				printf "%-22s %-28s %s\n" "$name" "$status" "$created"
			done
		else
			echo "No Docker VMs found"
		fi
	fi
	
	# Check if Vagrant is available
	if command -v vagrant &> /dev/null; then
		echo ""
		echo "📦 Vagrant VMs:"
		echo "---------------"
		vagrant global-status 2>/dev/null || echo "No Vagrant VMs found"
	fi
	
	echo ""
}

# Parse command line arguments manually for better control
CUSTOM_CONFIG=""
DEBUG_MODE=""
DRY_RUN=""
AUTO_LOGIN="true"
ARGS=()

# Manual argument parsing - much simpler and more reliable than getopt
while [[ $# -gt 0 ]]; do
	case "$1" in
		-c|--config)
			shift
			# Check if next argument exists and is not a flag or command
			if [[ $# -eq 0 ]] || [[ "$1" =~ ^- ]] || [[ "$1" =~ ^(init|generate|validate|list|temp|create|start|stop|restart|ssh|destroy|status|provision|logs|exec|kill|help)$ ]]; then
				# No argument provided or next is a flag/command - use scan mode
				CUSTOM_CONFIG="__SCAN__"
			else
				# Argument provided - use it as config path
				if [ -d "$1" ]; then
					CUSTOM_CONFIG="$1/vm.yaml"
				else
					CUSTOM_CONFIG="$1"
				fi
				shift
			fi
			;;
		-d|--debug)
			DEBUG_MODE="true"
			export VM_DEBUG="true"
			shift
			;;
		--dry-run)
			DRY_RUN="true"
			shift
			;;
		--auto-login)
			shift
			# Check if next argument exists and is a boolean value
			if [[ $# -gt 0 ]] && [[ "$1" =~ ^(true|false)$ ]]; then
				AUTO_LOGIN="$1"
				shift
			else
				# Default to true if no argument provided
				AUTO_LOGIN="true"
			fi
			;;
		--auto-login=*)
			# Handle --auto-login=true/false format
			AUTO_LOGIN="${1#*=}"
			if [[ ! "$AUTO_LOGIN" =~ ^(true|false)$ ]]; then
				echo "❌ Invalid value for --auto-login: $AUTO_LOGIN. Must be 'true' or 'false'." >&2
				exit 1
			fi
			shift
			;;
		-h|--help)
			show_usage
			exit 0
			;;
		-*)
			echo "❌ Unknown option: $1" >&2
			show_usage
			exit 1
			;;
		generate)
			# Special handling for generate command - pass all remaining args
			ARGS+=("$1")
			shift
			# Add all remaining arguments without parsing
			ARGS+=("$@")
			break
			;;
		test)
			# Special handling for test command - pass all remaining args
			ARGS+=("$1")
			shift
			# Add all remaining arguments without parsing
			ARGS+=("$@")
			break
			;;
		*)
			# Collect remaining arguments (command and its args)
			ARGS+=("$1")
			shift
			;;
	esac
done

# Restore positional parameters to the command and its arguments
set -- "${ARGS[@]}"

# Handle special commands
case "${1:-}" in
	"init")
		echo "✨ Creating new project configuration..."
		# Use validate-config.sh with special init flag
		if [ -n "$CUSTOM_CONFIG" ] && [ "$CUSTOM_CONFIG" != "__SCAN__" ]; then
			"$SCRIPT_DIR/validate-config.sh" --init "$CUSTOM_CONFIG"
		else
			"$SCRIPT_DIR/validate-config.sh" --init
		fi
		;;
	"generate")
		echo "⚙️ Generating configuration..."
		# Pass all remaining arguments to generate-config.sh
		shift
		"$SCRIPT_DIR/generate-config.sh" "$@"
		;;
	"validate")
		echo "✅ Validating configuration..."
		# Validate configuration using the centralized config manager
		if [ -n "$CUSTOM_CONFIG" ]; then
			"$SCRIPT_DIR/validate-config.sh" --validate "$CUSTOM_CONFIG"
		else
			"$SCRIPT_DIR/validate-config.sh" --validate
		fi
		;;
	"list")
		vm_list
		;;
	"kill")
		# Load config to determine provider
		CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR")
		if [ $? -ne 0 ]; then
			echo "❌ Invalid configuration"
			exit 1
		fi
		
		PROVIDER=$(get_provider "$CONFIG")
		
		if [ "$PROVIDER" = "docker" ]; then
			docker_kill "$CONFIG"
		else
			kill_virtualbox
		fi
		;;
	"temp")
		# Handle temp VM commands
		shift
		if [ $# -eq 0 ]; then
			echo "❌ Usage: vm temp ./folder1,./folder2,./folder3 [--auto-destroy]"
			echo "   Example: vm temp ./client,./server,./shared"
			echo "   Example: vm temp ./src --auto-destroy"
			echo "   Or: vm temp destroy"
			exit 1
		fi
		
		# Handle temp destroy subcommand
		if [ "$1" = "destroy" ]; then
			echo "🗑️ Destroying temporary VM..."
			# Try both old and new container names for compatibility
			if docker_cmd rm -f "vmtemp-dev" >/dev/null 2>&1 || docker_cmd rm -f "vm-temp" >/dev/null 2>&1; then
				# Also clean up volumes
				docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
				echo "✅ vm-temp destroyed successfully"
			else
				echo "❌ vm-temp not found or already destroyed"
			fi
			exit 0
		fi
		
		# Parse mount string from first argument (only if not destroy command)
		MOUNT_STRING="$1"
		shift
		
		# Check for --auto-destroy flag
		AUTO_DESTROY=""
		if [ "${1:-}" = "--auto-destroy" ]; then
			AUTO_DESTROY="true"
			shift
		fi
		
		if [ -z "$MOUNT_STRING" ]; then
			echo "❌ Usage: vm temp ./folder1,./folder2,./folder3 [--auto-destroy]"
			exit 1
		fi
		
		# Validate mount directories exist
		old_ifs="$IFS"
		IFS=','
		MOUNTS=($MOUNT_STRING)  # This is intentionally unquoted for word splitting
		IFS="$old_ifs"
		for mount in "${MOUNTS[@]}"; do
			mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
			if [[ "$mount" == *:* ]]; then
				mount="${mount%:*}"
			fi
			if [ ! -d "$mount" ]; then
				echo "❌ Error: Directory '$mount' does not exist" >&2
				exit 1
			fi
		done
		
		# Check if vm-temp already exists
		TEMP_CONTAINER="vmtemp-dev"  # Use consistent naming with regular VMs
		if docker_cmd inspect "$TEMP_CONTAINER" >/dev/null 2>&1; then
			echo "🔄 vm-temp already running - connecting..."
			# Use the standard docker_ssh function
			TEMP_CONFIG=$(cat <<EOF
{
  "project": {
    "name": "vmtemp",
    "hostname": "vm-temp.local",
    "workspace_path": "/workspace"
  },
  "vm": {
    "user": "developer"
  }
}
EOF
)
			docker_ssh "$TEMP_CONFIG" "" "."
			
			# Handle auto-destroy if flag was set
			if [ "$AUTO_DESTROY" = "true" ]; then
				echo "🗑️ Auto-destroying temp VM..."
				docker_cmd rm -f "$TEMP_CONTAINER" >/dev/null 2>&1
			fi
		else
			# Source the deep merge utility
			source "$SCRIPT_DIR/shared/deep-merge.sh"
			
			# Generate minimal temporary vm.yaml config with just overrides
			TEMP_CONFIG_FILE=$(mktemp /tmp/vm-temp.XXXXXX.json)
			# Ensure the temp file is removed when the script exits
			trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT
			
			cat > "$TEMP_CONFIG_FILE" <<EOF
{
  "project": {
    "name": "vmtemp",
    "hostname": "vm-temp.local"
  },
  "terminal": {
    "username": "vm-temp",
    "emoji": "🔧"
  },
  "services": {
    "postgresql": {
      "enabled": false
    },
    "redis": {
      "enabled": false
    },
    "mongodb": {
      "enabled": false
    }
  }
}
EOF
			
			# Extract schema defaults and merge with temp overrides
			SCHEMA_DEFAULTS=$("$SCRIPT_DIR/validate-config.sh" --extract-defaults "$SCRIPT_DIR/vm.schema.yaml")
			# Use yq to merge schema defaults with temp config
			CONFIG=$(yq -s '.[0] * .[1]' <(echo "$SCHEMA_DEFAULTS") "$TEMP_CONFIG_FILE")
			if [ $? -ne 0 ]; then
				echo "❌ Failed to generate temp VM configuration"
				rm -f "$TEMP_CONFIG_FILE"
				exit 1
			fi
			
			# Create a temporary project directory for docker-compose generation
			TEMP_PROJECT_DIR="/tmp/vm-temp-project-$$"
			mkdir -p "$TEMP_PROJECT_DIR"
			
			# Track the real paths for direct volume mounts
			MOUNT_MAPPINGS=()
			for mount in "${MOUNTS[@]}"; do
				mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
				if [[ "$mount" == *:* ]]; then
					mount="${mount%:*}"
				fi
				# Get absolute path
				REAL_PATH=$(realpath "$mount")
				MOUNT_NAME=$(basename "$mount")
				# Store the mapping for Docker volumes
				MOUNT_MAPPINGS+=("$REAL_PATH:$MOUNT_NAME")
			done
			
			# Export mount mappings for docker provisioning script
			export VM_TEMP_MOUNTS="${MOUNT_MAPPINGS[*]}"
			# Mark this as a temp VM so docker provisioning knows to skip the main mount
			export VM_IS_TEMP="true"
			
			# Store temp directory path for later cleanup
			# Use secure user-specific directory for marker file
			# Try XDG runtime dir first, then fall back to XDG state dir, then /tmp
			if [ -n "$XDG_RUNTIME_DIR" ] && mkdir -p "$XDG_RUNTIME_DIR/vm" 2>/dev/null; then
				MARKER_DIR="$XDG_RUNTIME_DIR/vm"
			elif mkdir -p "$HOME/.local/state/vm" 2>/dev/null; then
				MARKER_DIR="$HOME/.local/state/vm"
			else
				MARKER_DIR="/tmp"
				echo "⚠️  Warning: Using /tmp for marker files (less secure)"
			fi
			# Include container name for unique identification
			TEMP_DIR_MARKER="$MARKER_DIR/.vmtemp-${TEMP_CONTAINER}-marker"
			echo "$TEMP_PROJECT_DIR" > "$TEMP_DIR_MARKER"
			
			# Log the temporary directory creation for security audit
			if command -v logger >/dev/null 2>&1; then
				logger -t vm-temp "Created temp directory: $TEMP_PROJECT_DIR (marker: $TEMP_DIR_MARKER)"
			fi
			
			# Use the standard docker_up flow
			echo "🚀 Creating temporary VM with full provisioning..."
			docker_up "$CONFIG" "$TEMP_PROJECT_DIR" "true"
			
			# Clean up temp config file only (keep project dir for mounts)
			rm -f "$TEMP_CONFIG_FILE"
			
			# Handle auto-destroy if flag was set
			if [ "$AUTO_DESTROY" = "true" ]; then
				echo "🗑️ Auto-destroying temp VM..."
				docker_cmd rm -f "$TEMP_CONTAINER" >/dev/null 2>&1
				# Clean up volumes
				docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
				# Clean up temp project directory safely
				# Resolve real path to prevent directory traversal
				REAL_TEMP_DIR=$(realpath "$TEMP_PROJECT_DIR" 2>/dev/null)
				if [[ "$REAL_TEMP_DIR" == /tmp/vm-temp-project-* ]]; then
					rm -rf "$REAL_TEMP_DIR"
					# Log cleanup
					if command -v logger >/dev/null 2>&1; then
						logger -t vm-temp "Auto-destroyed temp directory: $REAL_TEMP_DIR"
					fi
				else
					echo "⚠️  Warning: Temp directory path resolved to unexpected location: $REAL_TEMP_DIR"
					# Log security event
					if command -v logger >/dev/null 2>&1; then
						logger -t vm-temp-security "ALERT: Rejected suspicious temp path during auto-destroy: $TEMP_PROJECT_DIR (resolved to: $REAL_TEMP_DIR)"
					fi
				fi
				rm -f "$TEMP_DIR_MARKER"
			fi
		fi
		;;
	"destroy")
		# Special handling for vm-temp
		if [ "${2:-}" = "vm-temp" ]; then
			echo "🗑️ Destroying temporary VM..."
			# Try both old and new container names for compatibility
			if docker_cmd rm -f "vmtemp-dev" >/dev/null 2>&1 || docker_cmd rm -f "vm-temp" >/dev/null 2>&1; then
				# Also clean up volumes
				docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
				
				# Clean up temp project directory if it exists
				# Look for marker file in secure location (same logic as creation)
				if [ -n "$XDG_RUNTIME_DIR" ] && [ -d "$XDG_RUNTIME_DIR/vm" ]; then
					MARKER_DIR="$XDG_RUNTIME_DIR/vm"
				elif [ -d "$HOME/.local/state/vm" ]; then
					MARKER_DIR="$HOME/.local/state/vm"
				else
					MARKER_DIR="/tmp"
				fi
				# Find marker file for this container
				TEMP_DIR_MARKER="$MARKER_DIR/.vmtemp-vmtemp-dev-marker"
				
				# Also check legacy location for backward compatibility
				if [ ! -f "$TEMP_DIR_MARKER" ] && [ -f "/tmp/.vmtemp-project-dir" ]; then
					TEMP_DIR_MARKER="/tmp/.vmtemp-project-dir"
					echo "⚠️  Warning: Using legacy marker file location"
				fi
				
				if [ -f "$TEMP_DIR_MARKER" ]; then
					TEMP_PROJECT_DIR=$(cat "$TEMP_DIR_MARKER")
					# Safety check: ensure it's a temp directory
					if [ -d "$TEMP_PROJECT_DIR" ]; then
						# Resolve the real path to prevent directory traversal attacks
						# Use realpath to follow symlinks and get the canonical path
						REAL_TEMP_DIR=$(realpath "$TEMP_PROJECT_DIR" 2>/dev/null)
						if [[ "$REAL_TEMP_DIR" == /tmp/vm-temp-project-* ]]; then
							# Additional safety: only remove if it contains symlinks (not real files)
							if find "$REAL_TEMP_DIR" -maxdepth 1 -type f | grep -q .; then
								echo "⚠️  Warning: Temp directory contains real files, not cleaning up: $TEMP_PROJECT_DIR"
								# Log security event
								if command -v logger >/dev/null 2>&1; then
									logger -t vm-temp-security "WARN: Refused to delete temp directory with real files: $TEMP_PROJECT_DIR"
								fi
							else
								rm -rf "$REAL_TEMP_DIR"
								echo "🧹 Cleaned up temporary project directory"
								# Log successful cleanup
								if command -v logger >/dev/null 2>&1; then
									logger -t vm-temp "Cleaned up temp directory: $REAL_TEMP_DIR"
								fi
							fi
						else
							echo "⚠️  Warning: Invalid temp directory path (resolved to $REAL_TEMP_DIR), not cleaning up: $TEMP_PROJECT_DIR"
							# Log security event
							if command -v logger >/dev/null 2>&1; then
								logger -t vm-temp-security "ALERT: Rejected suspicious temp path: $TEMP_PROJECT_DIR (resolved to: $REAL_TEMP_DIR)"
							fi
						fi
					else
						echo "⚠️  Warning: Temp directory not found: $TEMP_PROJECT_DIR"
					fi
					rm -f "$TEMP_DIR_MARKER"
				fi
				
				echo "✅ vm-temp destroyed successfully"
				
				# Clean up any stale marker files (older than 1 day)
				if [ -d "$MARKER_DIR" ]; then
					find "$MARKER_DIR" -name ".vmtemp-*-marker" -type f -mtime +1 -delete 2>/dev/null || true
				fi
			else
				echo "❌ vm-temp not found or already destroyed"
			fi
			exit 0
		fi
		
		# If no VM name provided, load config from current directory and destroy
		if [ $# -eq 1 ]; then
			# Load and validate config
			CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR")
			if [ $? -ne 0 ]; then
				echo "❌ No vm.yaml configuration file found. Run \"vm init\" to create one."
				exit 1
			fi
			
			PROVIDER=$(get_provider "$CONFIG")
			
			# Determine project directory
			if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
				PROJECT_DIR="$CURRENT_DIR"
			elif [ -n "$CUSTOM_CONFIG" ]; then
				FULL_CONFIG_PATH="$(cd "$CURRENT_DIR" && readlink -f "$CUSTOM_CONFIG")"
				PROJECT_DIR="$(dirname "$FULL_CONFIG_PATH")"
			else
				PROJECT_DIR="$CURRENT_DIR"
			fi
			
			# Get project name for confirmation
			project_name=$(echo "$CONFIG" | yq -r '.project.name' | tr -cd '[:alnum:]')
			container_name="${project_name}-dev"
			
			echo "⚠️  About to destroy VM: ${container_name}"
			echo -n "Are you sure? This will destroy the VM and all its data. (y/N): "
			read -r response
			case "$response" in
				[yY]|[yY][eE][sS])
					if [ "$PROVIDER" = "docker" ]; then
						docker_destroy "$CONFIG" "$PROJECT_DIR"
					else
						VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant destroy -f
					fi
					;;
				*)
					echo "❌ Destroy cancelled."
					exit 1
					;;
			esac
			exit 0
		fi
		# Fall through to default case for destroy with arguments
		;&
	"help"|"-h"|"--help"|"")
		show_usage
		;;
	*)
		# Load and validate config (discovery handled by validate-config.js)
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG main: CUSTOM_CONFIG='$CUSTOM_CONFIG'" >&2
		fi
		CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR")
		if [ $? -ne 0 ]; then
			echo "❌ Invalid configuration"
			exit 1
		fi
		
		PROVIDER=$(get_provider "$CONFIG")
		
		# Determine project directory and config path
		if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
			# Scan mode: project dir is where user ran the command
			PROJECT_DIR="$CURRENT_DIR"
			FULL_CONFIG_PATH=""
		elif [ -n "$CUSTOM_CONFIG" ]; then
			# If using custom config, project dir is where the config file is located
			# Resolve the path from the original directory where user ran the command
			FULL_CONFIG_PATH="$(cd "$CURRENT_DIR" && readlink -f "$CUSTOM_CONFIG")"
			PROJECT_DIR="$(dirname "$FULL_CONFIG_PATH")"
		else
			# Default: current directory, no explicit config path (uses discovery)
			PROJECT_DIR="$CURRENT_DIR"
			FULL_CONFIG_PATH=""
		fi
		
		# --- INSERT THIS VALIDATION LOGIC ---
		# Security: Validate that PROJECT_DIR is a legitimate project directory
		# and not a sensitive system path.
		if [[ -z "$PROJECT_DIR" ]] || [[ "$PROJECT_DIR" == "/" ]] || [[ "$PROJECT_DIR" == "/etc" ]] || [[ "$PROJECT_DIR" == "/usr" ]] || [[ "$PROJECT_DIR" == "/var" ]] || [[ "$PROJECT_DIR" == "/bin" ]] || [[ "$PROJECT_DIR" == "/sbin" ]]; then
			echo "❌ Error: Refusing to operate on critical system directory '$PROJECT_DIR'." >&2
			exit 1
		fi

		# Check for a project marker to prevent running in unintended locations.
		if [ ! -d "$PROJECT_DIR/.git" ] && [ ! -f "$PROJECT_DIR/vm.yaml" ] && [ ! -f "$PROJECT_DIR/vm.schema.yaml" ]; then
			echo "❌ Error: The specified directory '$PROJECT_DIR' does not appear to be a valid project root." >&2
			echo "   (Missing a .git directory, vm.yaml, or vm.schema.yaml file to act as a safeguard)." >&2
			exit 1
		fi
		# --- END OF VALIDATION LOGIC ---

		echo "🐳 Using provider: $PROVIDER"
		
		# Show dry run information if enabled
		if [ "$DRY_RUN" = "true" ]; then
			echo ""
			echo "🔍 DRY RUN MODE - showing what would be executed:"
			echo "   Project directory: $PROJECT_DIR"
			echo "   Provider: $PROVIDER"
			echo "   Command: $1"
			echo "   Arguments: ${@:2}"
			if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
				echo "   Config mode: Scanning up directory tree"
			elif [ -n "$CUSTOM_CONFIG" ]; then
				echo "   Config mode: Explicit config ($CUSTOM_CONFIG)"
			else
				echo "   Config mode: Default discovery"
			fi
			echo ""
			echo "🚫 Dry run complete - no commands were executed"
			exit 0
		fi
		
		# Route command to appropriate provider
		COMMAND="$1"
		shift
		
		if [ "$PROVIDER" = "docker" ]; then
			case "$COMMAND" in
				"create")
					# Check if VM already exists and confirm before recreating
					project_name=$(echo "$CONFIG" | yq -r '.project.name' | tr -cd '[:alnum:]')
					container_name="${project_name}-dev"
					
					if docker_cmd inspect "${container_name}" >/dev/null 2>&1; then
						echo "⚠️  VM '${container_name}' already exists."
						echo -n "Are you sure you want to recreate it? This will destroy the existing VM and all its data. (y/N): "
						read -r response
						case "$response" in
							[yY]|[yY][eE][sS])
								echo "🗑️  Destroying existing VM first..."
								docker_destroy "$CONFIG" "$PROJECT_DIR"
								;;
							*)
								echo "❌ VM creation cancelled."
								exit 1
								;;
						esac
					fi
					
					docker_up "$CONFIG" "$PROJECT_DIR" "$AUTO_LOGIN" "$@"
					;;
				"start")
					# Calculate relative path for start (same logic as SSH command)
					if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
						# In scan mode, we need to figure out where we are relative to the found config
						# Get the directory where vm.yaml was found from validate-config output
						CONFIG_DIR=$(echo "$CONFIG" | yq -r '.__config_dir // empty' 2>/dev/null)
						if [ "${VM_DEBUG:-}" = "true" ]; then
							echo "DEBUG start: CUSTOM_CONFIG='$CUSTOM_CONFIG'" >&2
							echo "DEBUG start: CONFIG_DIR='$CONFIG_DIR'" >&2
							echo "DEBUG start: CURRENT_DIR='$CURRENT_DIR'" >&2
						fi
						if [ -n "$CONFIG_DIR" ] && [ "$CONFIG_DIR" != "$CURRENT_DIR" ]; then
							# Calculate path from config dir to current dir
							RELATIVE_PATH=$(realpath --relative-to="$CONFIG_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
						else
							RELATIVE_PATH="."
						fi
					else
						# Normal mode: relative path from project dir to current dir
						RELATIVE_PATH=$(realpath --relative-to="$PROJECT_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
					fi
					if [ "${VM_DEBUG:-}" = "true" ]; then
						echo "DEBUG start: RELATIVE_PATH='$RELATIVE_PATH'" >&2
					fi
					docker_start "$CONFIG" "$PROJECT_DIR" "$RELATIVE_PATH" "$AUTO_LOGIN" "$@"
					;;
				"stop")
					docker_halt "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"restart")
					docker_reload "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"ssh")
					# Calculate relative path for SSH
					if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
						# In scan mode, we need to figure out where we are relative to the found config
						# Get the directory where vm.yaml was found from validate-config output
						CONFIG_DIR=$(echo "$CONFIG" | yq -r '.__config_dir // empty' 2>/dev/null)
						if [ -n "$CONFIG_DIR" ] && [ "$CONFIG_DIR" != "$CURRENT_DIR" ]; then
							# Calculate path from config dir to current dir
							RELATIVE_PATH=$(realpath --relative-to="$CONFIG_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
						else
							RELATIVE_PATH="."
						fi
					else
						# Normal mode: relative path from project dir to current dir
						RELATIVE_PATH=$(realpath --relative-to="$PROJECT_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
					fi
					
					if [ "${VM_DEBUG:-}" = "true" ]; then
						echo "DEBUG ssh: CURRENT_DIR='$CURRENT_DIR'" >&2
						echo "DEBUG ssh: PROJECT_DIR='$PROJECT_DIR'" >&2
						echo "DEBUG ssh: CUSTOM_CONFIG='$CUSTOM_CONFIG'" >&2
						echo "DEBUG ssh: CONFIG_DIR='$CONFIG_DIR'" >&2
						echo "DEBUG ssh: RELATIVE_PATH='$RELATIVE_PATH'" >&2
					fi
					
					# Get container name for connection message
					project_name=$(echo "$CONFIG" | yq -r '.project.name' | tr -cd '[:alnum:]')
					container_name="${project_name}-dev"
					echo "🎯 Connected to $container_name"
					
					docker_ssh "$CONFIG" "$PROJECT_DIR" "$RELATIVE_PATH" "$@"
					;;
				"destroy")
					docker_destroy "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"status")
					docker_status "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"provision")
					docker_provision "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"logs")
					docker_logs "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"exec")
					docker_exec "$CONFIG" "$@"
					;;
				"test")
					# Run tests using test-runner.sh
					"$SCRIPT_DIR/test-runner.sh" "$@"
					;;
				*)
					echo "❌ Unknown command for Docker provider: $COMMAND"
					exit 1
					;;
			esac
		else
			# Vagrant provider
			case "$COMMAND" in
				"create")
					# Check if VM already exists and confirm before recreating
					if VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant status default 2>/dev/null | grep -q "running\|poweroff\|saved"; then
						echo "⚠️  Vagrant VM already exists."
						echo -n "Are you sure you want to recreate it? This will destroy the existing VM and all its data. (y/N): "
						read -r response
						case "$response" in
							[yY]|[yY][eE][sS])
								echo "🗑️  Destroying existing VM first..."
								VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant destroy -f
								;;
							*)
								echo "❌ VM creation cancelled."
								exit 1
								;;
						esac
					fi
					
					# Start VM
					if [ -n "$FULL_CONFIG_PATH" ]; then
						VM_PROJECT_DIR="$PROJECT_DIR" VM_CONFIG="$FULL_CONFIG_PATH" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
					else
						VM_PROJECT_DIR="$PROJECT_DIR" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
					fi
					
					# Auto-SSH if enabled
					if [ "$AUTO_LOGIN" = "true" ]; then
						echo "🔗 Connecting to VM..."
						VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh
					else
						echo "💡 Use 'vm ssh' to connect to the VM"
					fi
					;;
				"ssh")
					# SSH into VM with relative path support
					# Calculate relative path (similar to Docker SSH logic)
					if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
						# In scan mode, figure out where we are relative to the found config
						CONFIG_DIR=$(echo "$CONFIG" | yq -r '.__config_dir // empty' 2>/dev/null)
						if [ -n "$CONFIG_DIR" ] && [ "$CONFIG_DIR" != "$CURRENT_DIR" ]; then
							RELATIVE_PATH=$(realpath --relative-to="$CONFIG_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
						else
							RELATIVE_PATH="."
						fi
					else
						# Normal mode: relative path from project dir to current dir
						RELATIVE_PATH=$(realpath --relative-to="$PROJECT_DIR" "$CURRENT_DIR" 2>/dev/null || echo ".")
					fi
					
					# Get workspace path from config
					WORKSPACE_PATH=$(echo "$CONFIG" | yq -r '.project.workspace_path // "/workspace"')
					
					if [ "$RELATIVE_PATH" != "." ]; then
						TARGET_DIR="${WORKSPACE_PATH}/${RELATIVE_PATH}"
						VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "cd '$TARGET_DIR' && exec /bin/zsh"
					else
						VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh
					fi
					;;
				"start")
					# Start existing VM (Vagrant equivalent of resume)
					VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant resume "$@" || VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
					;;
				"stop")
					# Stop VM but keep data
					VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant halt "$@"
					;;
				"restart")
					# Restart VM without reprovisioning
					VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant halt "$@"
					VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant resume "$@" || VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
					;;
				"exec")
					# Execute command in Vagrant VM
					VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "$@"
					;;
				"logs")
					# Show service logs in Vagrant VM
					echo "📋 Showing service logs (Ctrl+C to stop)..."
					VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "sudo journalctl -u postgresql -u redis-server -u mongod -f"
					;;
				"test")
					# Run tests using test-runner.sh
					"$SCRIPT_DIR/test-runner.sh" "$@"
					;;
				*)
					# Pass through to vagrant command
					if [ -n "$FULL_CONFIG_PATH" ]; then
						VM_PROJECT_DIR="$PROJECT_DIR" VM_CONFIG="$FULL_CONFIG_PATH" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant "$COMMAND" "$@"
					else
						VM_PROJECT_DIR="$PROJECT_DIR" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant "$COMMAND" "$@"
					fi
					;;
			esac
		fi
		;;
esac