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
	echo "  kill                 Force kill VM processes"
	echo ""
	echo "Examples:"
	echo "  $0 validate                    # Check configuration"
	echo "  $0 list                        # List all VM instances"
	echo "  $0 --config ./prod.json up     # Start VM with specific config"
	echo "  $0 --config up                 # Start VM scanning up for vm.json"
	echo "  $0 --dry-run ssh --config      # Show what would be executed"
	echo "  $0 up                          # Start the VM (auto-find vm.json)"
	echo "  $0 ssh                         # Connect to VM"
	echo "  $0 halt                        # Stop the VM"
	echo "  $0 kill                        # Kill stuck VM processes"
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

# Function to load and validate config (delegated to validate-config.js)
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
			echo "DEBUG load_config: Running: cd '$original_dir' && node '$SCRIPT_DIR/validate-config.js' '$config_path'" >&2
		fi
		(cd "$original_dir" && node "$SCRIPT_DIR/validate-config.js" "$config_path")
	else
		# Use default discovery logic - run from the original directory
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG load_config: Running: cd '$original_dir' && node '$SCRIPT_DIR/validate-config.js'" >&2
		fi
		(cd "$original_dir" && node "$SCRIPT_DIR/validate-config.js")
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
			docker compose "$@"
			;;
		"exec")
			docker exec "${container_name}" "$@"
			;;
		"exec-it")
			docker exec -it "${container_name}" "$@"
			;;
		*)
			cd "$project_dir"
			docker compose "$action" "$@"
			;;
	esac
}

# Docker functions
docker_up() {
	local config="$1"
	local project_dir="$2"
	shift 2
	
	echo "🐳 Starting Docker environment..."
	
	# Generate docker-compose.yml
	echo "$config" > /tmp/vm-config.json
	node "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.cjs" /tmp/vm-config.json "$project_dir"
	
	# Build and start containers
	docker_run "compose" "$config" "$project_dir" build
	docker_run "compose" "$config" "$project_dir" up -d "$@"
	
	# Get container name
	local project_name=$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')
	local container_name="${project_name}-dev"
	
	# Copy config file to container
	docker cp /tmp/vm-config.json "${container_name}:/tmp/vm-config.json"
	
	# Fix volume permissions before Ansible
	docker_run "exec" "$config" "$project_dir" chown -R vagrant:vagrant /home/vagrant/.nvm /home/vagrant/.cache
	
	# VM tool directory is already mounted read-only via docker-compose
	
	# Run Ansible playbook inside the container
	echo "🔧 Running Ansible provisioning..."
	
	# Check if debug mode is enabled
	ANSIBLE_VERBOSITY=""
	if [ "${VM_DEBUG:-}" = "true" ] || [ "${DEBUG:-}" = "true" ]; then
		echo "🐛 Debug mode enabled - showing detailed Ansible output"
		ANSIBLE_VERBOSITY="-vvv"
	fi
	
	# Create log file path
	ANSIBLE_LOG="/tmp/ansible-provision-$(date +%Y%m%d-%H%M%S).log"
	
	if docker_run "exec" "$config" "$project_dir" bash -c "ansible-playbook \
		-i localhost, \
		-c local \
		$ANSIBLE_VERBOSITY \
		--diff \
		/vm-tool/providers/vagrant/ansible/playbook.yml 2>&1 | tee $ANSIBLE_LOG"; then
		echo "✅ Ansible provisioning completed successfully!"
	else
		ANSIBLE_EXIT_CODE=$?
		echo "⚠️  Ansible provisioning had some issues (exit code: $ANSIBLE_EXIT_CODE)"
		echo "📋 Full log saved in container at: $ANSIBLE_LOG"
		echo "💡 Tips:"
		echo "   - Run with VM_DEBUG=true vm up to see detailed error output"
		echo "   - View the log: vm exec cat $ANSIBLE_LOG"
		echo "   - Or copy it: docker cp ${container_name}:$ANSIBLE_LOG ./ansible-error.log"
	fi
	
	# Ensure supervisor services are started
	echo "🔄 Starting services..."
	docker_run "exec" "$config" "$project_dir" bash -c "supervisorctl reread && supervisorctl update" || true
	
	# Clean up generated docker-compose.yml since containers are now running
	local compose_file="${project_dir}/docker-compose.yml"
	if [ -f "$compose_file" ]; then
		echo "🧹 Cleaning up generated docker-compose.yml..."
		rm "$compose_file"
	fi
	
	echo "✅ Docker environment is running and provisioned!"
	echo "🔗 Connecting to VM..."
	
	# Automatically SSH into the container  
	docker_ssh "$config" "" "."
}

docker_ssh() {
	local config="$1"
	local project_dir="$2"
	local relative_path="$3"
	shift 3
	
	# Get workspace path from config, default to /workspace
	local workspace_path=$(echo "$config" | jq -r '.project.workspace_path // "/workspace"')
	local target_dir="${workspace_path}"
	
	# If we have a relative path and it's not just ".", append it to workspace path
	if [ -n "$relative_path" ] && [ "$relative_path" != "." ]; then
		target_dir="${workspace_path}/${relative_path}"
	fi
	
	# Handle -c flag specifically for command execution
	if [ "$1" = "-c" ] && [ -n "$2" ]; then
		# Run command non-interactively
		docker_run "exec" "$config" "" su - vagrant -c "cd '$target_dir' && source ~/.zshrc && $2"
	elif [ $# -gt 0 ]; then
		# Run with all arguments
		docker_run "exec" "$config" "" su - vagrant -c "cd '$target_dir' && source ~/.zshrc && zsh $*"
	else
		# Interactive mode - use a simple approach that works
		local project_name=$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')
		local container_name="${project_name}-dev"
		
		# Run an interactive shell that properly handles signals
		docker exec -it "${container_name}" bash -c "
			cd '$target_dir'
			# Switch to vagrant user while preserving signal handling
			exec sudo -u vagrant -i bash -c \"cd '$target_dir' && exec /bin/zsh\"
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
	echo "🔧 Regenerating docker-compose.yml for destroy operation..."
	echo "$config" > /tmp/vm-config.json
	node "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.cjs" /tmp/vm-config.json "$project_dir"
	
	# Run docker compose down with volumes
	docker_run "down" "$config" "$project_dir" -v "$@"
	
	# Clean up the generated docker-compose.yml after destroy
	local compose_file="${project_dir}/docker-compose.yml"
	if [ -f "$compose_file" ]; then
		echo "🧹 Cleaning up generated docker-compose.yml..."
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
	
	echo "🔄 Rebuilding Docker environment..."
	
	# Generate fresh docker-compose.yml for provisioning
	echo "$config" > /tmp/vm-config.json
	node "$SCRIPT_DIR/providers/docker/docker-provisioning-simple.cjs" /tmp/vm-config.json "$project_dir"
	
	docker_run "compose" "$config" "$project_dir" build --no-cache
	docker_run "compose" "$config" "$project_dir" up -d "$@"
	
	# Clean up generated docker-compose.yml since containers are now running
	local compose_file="${project_dir}/docker-compose.yml"
	if [ -f "$compose_file" ]; then
		echo "🧹 Cleaning up generated docker-compose.yml..."
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
	echo "🔄 Stopping all Docker containers for this project..."
	local config="$1"
	local project_name=$(echo "$config" | jq -r '.project.name' | tr -cd '[:alnum:]')
	
	docker stop "${project_name}-dev" 2>/dev/null || true
	docker stop "${project_name}-postgres" 2>/dev/null || true
	docker stop "${project_name}-redis" 2>/dev/null || true
	docker stop "${project_name}-mongodb" 2>/dev/null || true
	
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
		local vm_containers=$(docker ps -a --format "{{.Names}}\t{{.Status}}\t{{.CreatedAt}}" | awk '$1 ~ /-dev$/ || $1 ~ /postgres/ || $1 ~ /redis/ || $1 ~ /mongodb/ {print}' 2>/dev/null || true)
		
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
			if [[ $# -eq 0 ]] || [[ "$1" =~ ^- ]] || [[ "$1" =~ ^(validate|list|up|ssh|halt|destroy|status|reload|provision|logs|exec|kill|help)$ ]]; then
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
	"validate")
		echo "🔍 Validating VM configuration..."
		# Validate configuration using the centralized config manager
		if [ -n "$CUSTOM_CONFIG" ]; then
			node "$SCRIPT_DIR/validate-config.js" --validate "$CUSTOM_CONFIG"
		else
			node "$SCRIPT_DIR/validate-config.js" --validate
		fi
		;;
	"list")
		vm_list
		;;
	"kill")
		# Load config to determine provider
		CONFIG=$(load_config "$CUSTOM_CONFIG" "$CURRENT_DIR")
		if [ $? -ne 0 ]; then
			echo "❌ Configuration validation failed. Aborting."
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
			echo "❌ Configuration validation failed. Aborting."
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
		
		echo "🔧 Using provider: $PROVIDER"
		
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