#!/bin/bash
# VM wrapper script for Goobits - supports both Vagrant and Docker
# Usage: ./packages/vm/vm.sh [command] [args...]

set -e

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
	echo "Usage: $0 [--config [PATH]] [--debug] [--dry-run] [command] [args...]"
	echo ""
	echo "Options:"
	echo "  --config [PATH]      Use specific vm.json file, or scan up directory tree if no path given"
	echo "  --debug              Enable debug output"
	echo "  --dry-run            Show what would be executed without actually running it"
	echo ""
	echo "Commands:"
	echo "  init                  Initialize a new vm.json configuration file"
	echo "  generate              Generate vm.json by composing services"
	echo "  validate              Validate VM configuration"
	echo "  list                  List all VM instances"
	echo "  up [args]            Start VM"
	echo "  ssh [args]           SSH into VM"
	echo "  halt [args]          Stop VM"
	echo "  destroy [args]       Destroy VM"
	echo "  status [args]        Check VM status"
	echo "  reload [args]        Reload VM"
	echo "  provision [args]     Reprovision VM"
	echo "  logs [args]          View VM logs (Docker only)"
	echo "  exec [args]          Execute command in VM (Docker only)"
	echo "  test [args]          Run VM test suite"
	echo "  kill                 Force kill VM processes"
	echo ""
	echo "Examples:"
	echo "  $0 generate --services postgresql,redis  # Generate config with services"
	echo "  $0 generate --ports 3020 --name my-app   # Generate with custom ports/name"
	echo "  $0 validate                              # Check configuration"
	echo "  $0 list                                  # List all VM instances"
	echo "  $0 --config ./prod.json up               # Start VM with specific config"
	echo "  $0 --config up                           # Start VM scanning up for vm.json"
	echo "  $0 up                                    # Start the VM (auto-find vm.json)"
	echo "  $0 ssh                                   # Connect to VM"
	echo "  $0 halt                                  # Stop the VM"
	echo ""
	echo "The provider (Vagrant or Docker) is determined by the 'provider' field in vm.json"
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
			echo "DEBUG load_config: Running: cd '$original_dir' && '$SCRIPT_DIR/validate-config.sh' '$config_path'" >&2
		fi
		(cd "$original_dir" && "$SCRIPT_DIR/validate-config.sh" "$config_path")
	else
		# Use default discovery logic - run from the original directory
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG load_config: Running: cd '$original_dir' && '$SCRIPT_DIR/validate-config.sh'" >&2
		fi
		(cd "$original_dir" && "$SCRIPT_DIR/validate-config.sh")
	fi
}

# Get provider from config
get_provider() {
	local config="$1"
	echo "$config" | jq -r '.provider // "docker"'
}

# Docker helper function to reduce duplication
docker_run() {
	local action="$1"
	local config="$2"
	local project_dir="$3"
	shift 3
	
	# Extract project name once
	local project_name=$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')
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
	shift 2
	
	echo "🚀 Starting development environment..."
	
	# Generate docker-compose.yml
	echo "$config" > /tmp/vm-config.json
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" /tmp/vm-config.json "$project_dir"
	
	# Build and start containers
	docker_run "compose" "$config" "$project_dir" build
	docker_run "compose" "$config" "$project_dir" up -d "$@"
	
	# Get container name
	local project_name=$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')
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
	if docker_cmd cp /tmp/vm-config.json "${container_name}:/tmp/vm-config.json"; then
		echo "✅ Configuration loaded"
	else
		echo "❌ Configuration loading failed"
		return 1
	fi
	
	# Fix volume permissions before Ansible
	echo "🔑 Setting up permissions..."
	local project_user=$(echo "$config" | jq -r '.vm.user // "developer"')
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
		echo "   - Run with VM_DEBUG=true vm up to see detailed error output"
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
	echo "🌟 Entering development environment..."
	
	# Automatically SSH into the container  
	docker_ssh "$config" "" "."
}

docker_ssh() {
	local config="$1"
	local project_dir="$2"
	local relative_path="$3"
	shift 3
	
	# Get workspace path and user from config
	local workspace_path=$(echo "$config" | jq -r '.project.workspace_path // "/workspace"')
	local project_user=$(echo "$config" | jq -r '.vm.user // "developer"')
	local target_dir="${workspace_path}"
	
	# If we have a relative path and it's not just ".", append it to workspace path
	if [ -n "$relative_path" ] && [ "$relative_path" != "." ]; then
		target_dir="${workspace_path}/${relative_path}"
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
		local project_name=$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')
		local container_name="${project_name}-dev"
		
		# Run an interactive shell that properly handles signals
		docker_cmd exec -it "${container_name}" bash -c "
			cd '$target_dir'
			# Switch to project user while preserving signal handling
			exec sudo -u $project_user -i bash -c \"cd '$target_dir' && exec /bin/zsh\"
		"
	fi
}

docker_halt() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	docker_run "stop" "$config" "$project_dir" "$@"
}

docker_destroy() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	# Generate docker-compose.yml temporarily for destroy operation
	echo "🧹 Preparing cleanup..."
	echo "$config" > /tmp/vm-config.json
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" /tmp/vm-config.json "$project_dir"
	
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
	docker_up "$config" "$project_dir" "$@"
}

docker_provision() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	echo "🔄 Rebuilding environment..."
	
	# Generate fresh docker-compose.yml for provisioning
	echo "$config" > /tmp/vm-config.json
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" /tmp/vm-config.json "$project_dir"
	
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
	local project_name=$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')
	
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
ARGS=()

# Manual argument parsing - much simpler and more reliable than getopt
while [[ $# -gt 0 ]]; do
	case "$1" in
		-c|--config)
			shift
			# Check if next argument exists and is not a flag or command
			if [[ $# -eq 0 ]] || [[ "$1" =~ ^- ]] || [[ "$1" =~ ^(init|generate|validate|list|up|ssh|halt|destroy|status|reload|provision|logs|exec|kill|help)$ ]]; then
				# No argument provided or next is a flag/command - use scan mode
				CUSTOM_CONFIG="__SCAN__"
			else
				# Argument provided - use it as config path
				if [ -d "$1" ]; then
					CUSTOM_CONFIG="$1/vm.json"
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
				"up")
					docker_up "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"ssh")
					# Calculate relative path for SSH
					if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
						# In scan mode, we need to figure out where we are relative to the found config
						# Get the directory where vm.json was found from validate-config.js output
						CONFIG_DIR=$(echo "$CONFIG" | jq -r '.__config_dir // empty' 2>/dev/null)
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
					docker_ssh "$CONFIG" "$PROJECT_DIR" "$RELATIVE_PATH" "$@"
					;;
				"halt")
					docker_halt "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"destroy")
					docker_destroy "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"status")
					docker_status "$CONFIG" "$PROJECT_DIR" "$@"
					;;
				"reload")
					docker_reload "$CONFIG" "$PROJECT_DIR" "$@"
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
				"up")
					# Start VM and auto-SSH
					if [ -n "$FULL_CONFIG_PATH" ]; then
						VM_PROJECT_DIR="$PROJECT_DIR" VM_CONFIG="$FULL_CONFIG_PATH" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
					else
						VM_PROJECT_DIR="$PROJECT_DIR" VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant up "$@"
					fi
					echo "🔗 Connecting to VM..."
					VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh
					;;
				"ssh")
					# SSH into VM with relative path support
					# Calculate relative path (similar to Docker SSH logic)
					if [ "$CUSTOM_CONFIG" = "__SCAN__" ]; then
						# In scan mode, figure out where we are relative to the found config
						CONFIG_DIR=$(echo "$CONFIG" | jq -r '.__config_dir // empty' 2>/dev/null)
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
					WORKSPACE_PATH=$(echo "$CONFIG" | jq -r '.project.workspace_path // "/workspace"')
					
					if [ "$RELATIVE_PATH" != "." ]; then
						TARGET_DIR="${WORKSPACE_PATH}/${RELATIVE_PATH}"
						VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh -c "cd '$TARGET_DIR' && exec /bin/zsh"
					else
						VAGRANT_CWD="$SCRIPT_DIR/providers/vagrant" vagrant ssh
					fi
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