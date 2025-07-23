#!/bin/bash
# VM Temporary VM Management Module
# Extracted from vm.sh to improve maintainability

# Temp VM state management constants and functions
TEMP_STATE_DIR="$HOME/.vm"
TEMP_STATE_FILE="$TEMP_STATE_DIR/temp-vm.state"

# Wrapper function for yq to handle different versions
# This system has Python yq (kislyuk/yq) which outputs JSON, not raw strings
yq_raw() {
	local filter="$1"
	local file="$2"
	yq "$filter" "$file" 2>/dev/null | jq -r '.' 2>/dev/null || echo ""
}

# Validate directory name for security
validate_directory_name() {
	local dir="$1"
	
	# Check for dangerous characters that could cause shell injection
	if [[ "$dir" =~ [\;\`\$\"] ]]; then
		echo "❌ Error: Directory name contains potentially dangerous characters"
		echo "💡 Directory names cannot contain: ; \` $ \""
		return 1
	fi
	
	# Check for directory traversal attempts
	if [[ "$dir" =~ \.\./|/\.\. ]]; then
		echo "❌ Error: Directory path traversal not allowed"
		return 1
	fi
	
	return 0
}

# Get container name from state file with comprehensive error handling
get_container_name() {
	if [ ! -f "$TEMP_STATE_FILE" ]; then
		echo "❌ No temp VM found" >&2
		echo "💡 Create one with: vm temp ./your-directory" >&2
		return 1
	fi

	# Validate state file integrity
	if ! validate_state_file; then
		return 1
	fi

	local container_name=""
	if command -v yq &> /dev/null; then
		container_name=$(yq_raw '.container_name // empty' "$TEMP_STATE_FILE")
	else
		container_name=$(grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
	fi

	if [ -z "$container_name" ]; then
		echo "❌ Error: Could not read container name from state file" >&2
		echo "📁 State file: $TEMP_STATE_FILE" >&2
		return 1
	fi

	echo "$container_name"
}

# Validate that temp VM state file exists
require_temp_vm() {
	if [ ! -f "$TEMP_STATE_FILE" ]; then
		echo "❌ No active temp VM found"
		echo "💡 Create one with: vm temp ./your-directory"
		return 1
	fi
	
	# Validate state file integrity
	if ! validate_state_file; then
		return 1
	fi
	
	return 0
}

# Add temp file cleanup with trap handlers
setup_temp_file_cleanup() {
	local temp_file="$1"
	trap "rm -f '$temp_file' 2>/dev/null" EXIT INT TERM
}

# Validate state file is not corrupted
validate_state_file() {
	if [ ! -f "$TEMP_STATE_FILE" ]; then
		return 1
	fi

	# Test if file is valid YAML
	if command -v yq &> /dev/null; then
		if ! yq . "$TEMP_STATE_FILE" >/dev/null 2>&1; then
			echo "❌ State file is corrupted or invalid YAML" >&2
			echo "📁 File: $TEMP_STATE_FILE" >&2
			echo "💡 Try 'vm temp destroy' to clean up" >&2
			return 1
		fi
	fi

	return 0
}

# Standard error message functions for consistency
vm_not_found_error() {
	echo "❌ No temp VM found"
	echo "💡 Create one with: vm temp ./your-directory"
}

vm_not_running_error() {
	echo "❌ Temp VM is not running"
	echo "💡 Start it with: vm temp start"
}

# Save temporary VM state
save_temp_state() {
	local container_name="$1"
	local mounts="$2"
	local project_dir="$3"
	
	# Create state directory if it doesn't exist
	mkdir -p "$TEMP_STATE_DIR"
	
	# Create state file with YAML format
	cat > "$TEMP_STATE_FILE" <<-EOF
	container_name: $container_name
	created_at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
	project_dir: $project_dir
	mounts:
	EOF
	
	# Add mounts to the state file
	if [ -n "$mounts" ]; then
		# Split mounts by comma and add to state file
		local old_ifs="$IFS"
		IFS=','
		local MOUNT_ARRAY=($mounts)
		IFS="$old_ifs"
		
		for mount in "${MOUNT_ARRAY[@]}"; do
			# Trim whitespace
			mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
			# Get absolute path for the source
			local source="${mount%:*}"
			local perm="rw"  # default
			if [[ "$mount" == *:* ]]; then
				perm="${mount##*:}"
			fi
			local abs_source="$(cd "$project_dir" && realpath "$source" 2>/dev/null || echo "$source")"
			local target="/workspace/$(basename "$source")"
			
			# Use new explicit format
			cat >> "$TEMP_STATE_FILE" <<-EOF
			  - source: $abs_source
			    target: $target
			    permissions: $perm
			EOF
		done
	fi
}

# Read temporary VM state
read_temp_state() {
	if [ ! -f "$TEMP_STATE_FILE" ]; then
		return 1
	fi
	
	# Check if yq is available for parsing YAML
	if command -v yq &> /dev/null; then
		yq_raw '.' "$TEMP_STATE_FILE"
	else
		# Fallback to cat if yq is not available
		cat "$TEMP_STATE_FILE"
	fi
}

# Get temp VM container name from state
get_temp_container_name() {
	echo "DEBUG: Inside get_temp_container_name at $(date)" >> /tmp/vm-temp-debug.log
	if [ ! -f "$TEMP_STATE_FILE" ]; then
		echo "DEBUG: State file not found" >> /tmp/vm-temp-debug.log
		echo ""
		return 1
	fi
	
	if command -v yq &> /dev/null; then
		yq_raw '.container_name // empty' "$TEMP_STATE_FILE"
	else
		# Fallback to grep if yq is not available
		grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//'
	fi
}

# Check if temp VM is running
is_temp_vm_running() {
	local container_name="$1"
	if [ -z "$container_name" ]; then
		return 1
	fi
	
	docker_cmd inspect "$container_name" >/dev/null 2>&1
}

# Get mounts from state file
get_temp_mounts() {
	if [ ! -f "$TEMP_STATE_FILE" ]; then
		echo ""
		return 1
	fi
	
	if command -v yq &> /dev/null; then
		# Handle both old and new formats
		# First try old format (simple string)
		local old_format=$(yq -r '.mounts[]? // empty' "$TEMP_STATE_FILE" 2>/dev/null | grep -E '^[^:]+:[^:]+:[^:]+$' | tr '\n' ',' | sed 's/,$//')
		if [ -n "$old_format" ]; then
			echo "$old_format"
		else
			# New format - construct mount string
			yq -r '.mounts[]? | "\(.source):\(.target):\(.permissions)"' "$TEMP_STATE_FILE" 2>/dev/null | tr '\n' ',' | sed 's/,$//'
		fi
	else
		# Fallback to awk if yq is not available
		# This will work for old format, new format requires parsing
		awk '/^mounts:/{flag=1; next} /^[^ ]/{flag=0} flag && /^  - /{print}' "$TEMP_STATE_FILE" 2>/dev/null | 
			awk -F': ' '/source:/{s=$2} /target:/{t=$2} /permissions:/{p=$2; print s":"t":"p}' | 
			tr '\n' ',' | sed 's/,$//'
	fi
}

# Compare two mount strings for equality
compare_mounts() {
	local existing="$1"
	local requested="$2"
	
	# Sort both mount strings and compare
	local sorted_existing=$(echo "$existing" | tr ',' '\n' | sort | tr '\n' ',' | sed 's/,$//')
	local sorted_requested=$(echo "$requested" | tr ',' '\n' | sort | tr '\n' ',' | sed 's/,$//')
	
	[ "$sorted_existing" = "$sorted_requested" ]
}

# Clean up orphaned Docker resources from previous temp VM runs
cleanup_orphaned_temp_resources() {
	# Remove orphaned temp VM networks that don't have running containers  
	{
		docker network ls --filter "name=vm-temp-project" -q | while read network_id; do
			# Check if any container is using this network
			if ! docker ps -q --filter "network=$network_id" | grep -q .; then
				docker network rm "$network_id" 2>/dev/null || true
			fi
		done
		
		# Remove orphaned temp VM volumes that aren't in use
		docker volume ls --filter "name=vm-temp-project" -q | xargs -r docker volume rm 2>/dev/null || true
	} >/dev/null 2>&1
	
	# Clean up old vmtemp volumes if no vmtemp container exists
	if ! docker ps -a --filter "name=vmtemp" -q | grep -q .; then
		docker volume rm vmtemp_nvm vmtemp_cache vmtemp_config 2>/dev/null || true
	fi
}

# Recreate temp VM with updated mounts
recreate_temp_vm_with_mounts() {
	local container_name="$1"
	local start_time=$(date +%s)
	
	echo "🔄 Recreating container with updated mounts..."
	
	# Get current state
	local project_dir=$(yq_raw '.project_dir' "$TEMP_STATE_FILE")
	if [ -z "$project_dir" ]; then
		project_dir=""
	fi
	if [ -z "$project_dir" ]; then
		echo "❌ Error: Could not read project directory from state file"
		return 1
	fi
	
	# Read current mount configuration
	local mount_string=""
	if command -v yq &> /dev/null; then
		# Check format and build mount string
		if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
			# New format - reconstruct mount string with permissions
			mount_string=$(yq -r '.mounts[] | "\(.source):\(.permissions)"' "$TEMP_STATE_FILE" 2>/dev/null | tr '\n' ',' | sed 's/,$//')
		else
			# Old format
			mount_string=$(yq -r '.mounts[]' "$TEMP_STATE_FILE" 2>/dev/null | tr '\n' ',' | sed 's/,$//')
		fi
	fi
	
	echo "🛑 Stopping container..."
	docker_cmd stop "$container_name" > /dev/null 2>&1 || true
	
	echo "🗑️  Removing old container..."
	docker_cmd rm -f "$container_name" > /dev/null 2>&1 || true
	
	# Set environment variable for mounts
	export VM_TEMP_MOUNTS="$mount_string"
	
	# Create the container configuration
	local config=$(yq -n \
		--arg project_name "vm-temp-project" \
		--arg container_name "$container_name" \
		--arg init_script "/opt/provision.sh" \
		'{
			"project": {
				"name": $project_name,
				"type": "single"
			},
			"settings": {
				"ssh_port": 2222,
				"providers": "docker"
			},
			"environments": {
				"dev": {
					"type": "ubuntu",
					"providers": {
						"docker": {
							"image": "vm-ubuntu-24.04:latest",
							"container_name": $container_name,
							"init_script": $init_script,
							"privileged": true,
							"network_mode": "bridge",
							"volumes": [
								{
									"type": "volume",
									"source": "vmtemp_home",
									"target": "/home/developer"
								}
							]
						}
					}
				}
			}
		}')
	
	echo "🚀 Creating container with new mounts..."
	
	# Use docker-compose to recreate the container
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" <(echo "$config") "$project_dir"
	
	# Start the new container
	docker_cmd start "$container_name" > /dev/null 2>&1
	
	# Wait for container to be ready
	echo "⏳ Waiting for container to be ready..."
	local max_attempts=30
	local attempt=1
	while [ $attempt -le $max_attempts ]; do
		if docker_cmd exec "$container_name" test -f /tmp/provisioning_complete 2>/dev/null; then
			break
		fi
		sleep 1
		((attempt++))
	done
	
	local end_time=$(date +%s)
	local elapsed=$((end_time - start_time))
	
	echo "✅ Container recreated with updated mounts in ${elapsed} seconds"
	return 0
}

# Handle temp VM commands
handle_temp_command() {
	local args=("$@")
	
	# Add debug output for temp command
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: temp command called with args: $*" >&2
		echo "DEBUG: Current directory: $(pwd)" >&2
		echo "DEBUG: SCRIPT_DIR: $SCRIPT_DIR" >&2
	fi
	
	# Check if Docker is available
	if ! command -v docker &> /dev/null; then
		echo "❌ Docker is required for temporary VMs but is not installed"
		echo "💡 Install Docker to use temp VMs: https://docs.docker.com/get-docker/"
		exit 1
	fi
	
	# Check if Docker daemon is running
	if ! docker_cmd version >/dev/null 2>&1; then
		echo "❌ Docker daemon is not running or not accessible"
		echo "💡 Tips:"
		echo "   - Start Docker Desktop (if on macOS/Windows)"
		echo "   - Run: sudo systemctl start docker (if on Linux)"
		echo "   - Check permissions: groups | grep docker"
		exit 1
	fi
	
	# Show help if no arguments or --help flag
	if [ $# -eq 0 ] || [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
		echo "❌ Usage: vm temp <command> [options]"
		echo ""
		echo "Commands:"
		echo "  <mounts>              Create/connect temp VM with mounts"
		echo "  ssh                   SSH into the active temp VM"
		echo "  status                Show status of the active temp VM"
		echo "  destroy               Destroy the active temp VM"
		echo "  mount <path>          Add a mount to running temp VM"
		echo "  unmount <path>        Remove a mount from running temp VM"
		echo "  unmount --all         Remove all mounts and destroy temp VM"
		echo "  mounts                List current mounts"
		echo "  list                  List active temp VM instances"
		echo ""
		echo "Lifecycle Commands:"
		echo "  start                 Start stopped temp VM"
		echo "  stop                  Stop temp VM (preserves state)"
		echo "  restart               Restart temp VM"
		echo ""
		echo "Management Commands:"
		echo "  provision             Re-run provisioning"
		echo "  logs                  View container logs"
		echo ""
		echo "Examples:"
		echo "  vm temp ./src ./tests           # Mount multiple directories"
		echo "  vm temp ./src:rw ./config:ro    # With permissions"
		echo "  vm temp ./app --auto-destroy    # Auto-cleanup on exit"
		echo "  vm temp ssh                     # Connect to existing temp VM"
		echo "  vm temp mount ./docs            # Add mount to running VM"
		echo "  vm temp list                    # List active temp VMs"
		echo "  vm temp mounts                  # Show current mounts"
		echo "  vm temp unmount ./src           # Remove specific mount"
		echo "  vm temp unmount --all           # Remove all mounts and cleanup"
		echo "  vm temp stop                    # Stop temp VM"
		echo "  vm temp start                   # Start stopped temp VM"
		echo "  vm temp logs -f                 # View and follow container logs"
		echo ""
		echo "Mount Permissions:"
		echo "  :rw  Read-write access (default)"
		echo "  :ro  Read-only access"
		echo ""
		echo "Flags:"
		echo "  --yes, -y             Skip confirmation prompts"
		exit 1
	fi
	
	# Handle subcommands
	case "$1" in
		"destroy")
			# Destroy temp VM
			require_temp_vm || exit 0
			
			# Inline container name retrieval to avoid command substitution issues
			container_name=""
			if [ -f "$TEMP_STATE_FILE" ]; then
				if command -v yq &> /dev/null; then
					container_name=$(yq_raw '.container_name // empty' "$TEMP_STATE_FILE")
				else
					container_name=$(grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
				fi
			fi
			echo "🗑️ Destroying temp VM: $container_name"
			docker_cmd rm -f "$container_name" >/dev/null 2>&1
			# Clean up volumes
			docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
			# Remove state file
			rm -f "$TEMP_STATE_FILE"
			echo "✅ Temp VM destroyed"
			exit 0
			;;
		"ssh")
			shift
			# SSH into temp VM
			require_temp_vm || exit 1
			
			# Inline container name retrieval to avoid command substitution issues
			container_name=""
			if [ -f "$TEMP_STATE_FILE" ]; then
				if command -v yq &> /dev/null; then
					container_name=$(yq_raw '.container_name // empty' "$TEMP_STATE_FILE")
				else
					container_name=$(grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
				fi
			fi
			if ! is_temp_vm_running "$container_name"; then
				echo "❌ Temp VM exists but is not running"
				echo "💡 Check status with: vm temp status"
				exit 1
			fi
			
			echo "🔗 Connecting to $container_name..."
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
			docker_ssh "$TEMP_CONFIG" "" "." "$@"
			exit 0
			;;
		"status")
			# Show temp VM status
			require_temp_vm || exit 0
			
			# Inline container name retrieval to avoid command substitution issues
			container_name=""
			if [ -f "$TEMP_STATE_FILE" ]; then
				if command -v yq &> /dev/null; then
					container_name=$(yq_raw '.container_name // empty' "$TEMP_STATE_FILE")
				else
					container_name=$(grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
				fi
			fi
			echo "📋 Temp VM Status:"
			echo "=================="
			
			if command -v yq &> /dev/null; then
				echo "Container: $(yq_raw '.container_name' "$TEMP_STATE_FILE")"
				echo "Created: $(yq_raw '.created_at' "$TEMP_STATE_FILE")"
				echo "Project: $(yq_raw '.project_dir' "$TEMP_STATE_FILE")"
				echo ""
				echo "Mounts:"
				# Try to detect format and display accordingly
				if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
					# New format
					yq -r '.mounts[]? | "  • \(.source) → \(.target) [\(.permissions)]"' "$TEMP_STATE_FILE" 2>/dev/null
				else
					# Old format
					yq -r '.mounts[]?' "$TEMP_STATE_FILE" 2>/dev/null | while read -r mount; do
						echo "  • $mount"
					done
				fi
			else
				# Fallback display without yq
				cat "$TEMP_STATE_FILE"
			fi
			
			# Check if container is actually running
			if is_temp_vm_running "$container_name"; then
				echo ""
				echo "Status: ✅ Running"
			else
				echo ""
				echo "Status: ❌ Not running (state file exists but container is gone)"
			fi
			exit 0
			;;
		"mount")
			# Add a new mount to running temp VM
			shift
			if [ $# -eq 0 ]; then
				echo "❌ Usage: vm temp mount <path>[:ro|:rw] [--yes]"
				exit 1
			fi
			
			# Check if temp VM exists
			require_temp_vm || exit 1
			
			# Get existing container
			local container_name
			container_name=$(get_container_name) || exit 1
			
			if ! is_temp_vm_running "$container_name"; then
				echo "🚀 Temp VM is stopped. Starting it first..."
				docker_cmd start "$container_name"
				
				# Wait for ready
				echo "⏳ Waiting for container to be ready..."
				local max_attempts=15
				local attempt=1
				while [ $attempt -le $max_attempts ]; do
					if docker_cmd exec "$container_name" echo "ready" >/dev/null 2>&1; then
						echo "✅ Temp VM started! Now adding mount..."
						break
					fi
					sleep 1
					((attempt++))
				done
				
				if [ $attempt -gt $max_attempts ]; then
					echo "❌ Failed to start temp VM"
					exit 1
				fi
			fi
			
			# Parse arguments
			local NEW_MOUNT="$1"
			local AUTO_YES=""
			shift
			
			# Check for --yes flag
			while [[ $# -gt 0 ]]; do
				case "$1" in
					--yes|-y)
						AUTO_YES="true"
						shift
						;;
					*)
						echo "❌ Unknown option: $1"
						exit 1
						;;
				esac
			done
			
			# Parse and validate the new mount
			local source=""
			local perm="rw"
			
			# Parse permission suffix if present
			if [[ "$NEW_MOUNT" =~ ^(.+):(ro|rw)$ ]]; then
				source="${BASH_REMATCH[1]}"
				perm="${BASH_REMATCH[2]}"
			elif [[ "$NEW_MOUNT" == *:* ]]; then
				# Invalid permission format
				echo "❌ Error: Invalid permission format in '$NEW_MOUNT'"
				echo "💡 Valid permissions are :ro (read-only) or :rw (read-write)"
				exit 1
			else
				source="$NEW_MOUNT"
			fi
			
			# Validate source directory exists
			if [ ! -d "$source" ]; then
				echo "❌ Error: Directory '$source' does not exist"
				exit 1
			fi
			
			# Validate directory name for security
			if ! validate_directory_name "$source"; then
				exit 1
			fi
			
			# Get absolute path
			local abs_source=$(cd "$source" && pwd)
			local target="/workspace/$(basename "$abs_source")"
			
			# Check if already mounted
			if command -v yq &> /dev/null; then
				if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
					# New format
					if yq -r '.mounts[] | select(.source == "'"$abs_source"'") | .source' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "$abs_source"; then
						echo "❌ Directory '$abs_source' is already mounted"
						exit 1
					fi
				fi
			fi
			
			# Show warning and get confirmation
			if [ "$AUTO_YES" != "true" ]; then
				echo "⚠️  This will restart the container (takes ~5 seconds)"
				echo "   Your work will be preserved. Continue? (y/N): "
				read -r response
				case "$response" in
					[yY]|[yY][eE][sS])
						;;
					*)
						echo "❌ Operation cancelled"
						exit 1
						;;
				esac
			fi
			
			# Add mount to state file
			echo "📝 Updating mount configuration..."
			if command -v yq &> /dev/null; then
				# Create temporary file for the update
				local temp_file=$(mktemp)
				setup_temp_file_cleanup "$temp_file"
				cp "$TEMP_STATE_FILE" "$temp_file"
				
				# Add the new mount in new format - SECURE: use yq --arg to prevent shell injection
				yq --arg source "$abs_source" --arg target "$target" --arg perm "$perm" \
					'.mounts += [{"source": $source, "target": $target, "permissions": $perm}]' \
					"$temp_file" > "$TEMP_STATE_FILE"
				
				rm -f "$temp_file"
			else
				# Fallback: append manually
				cat >> "$TEMP_STATE_FILE" <<-EOF
				  - source: $abs_source
				    target: $target
				    permissions: $perm
				EOF
			fi
			
			# Recreate the container
			recreate_temp_vm_with_mounts "$container_name"
			
			echo ""
			echo "📁 Mount added successfully:"
			echo "   $abs_source → $target ($perm)"
			exit 0
			;;
		"unmount")
			# Remove a mount from running temp VM
			shift
			if [ $# -eq 0 ]; then
				echo "❌ Usage: vm temp unmount <path> [--yes]"
				echo "❌ Usage: vm temp unmount --all [--yes]"
				exit 1
			fi
			
			# Get existing container
			local container_name
			container_name=$(get_container_name) || exit 1
			
			if ! is_temp_vm_running "$container_name"; then
				echo "❌ Temp VM exists but is not running"
				exit 1
			fi
			
			# Handle --all flag first
			if [ "$1" = "--all" ]; then
				shift
				local AUTO_YES=""
				
				# Check for --yes flag after --all
				while [[ $# -gt 0 ]]; do
					case "$1" in
						--yes|-y)
							AUTO_YES="true"
							shift
							;;
						*)
							echo "❌ Unknown option: $1"
							exit 1
							;;
					esac
				done
				
				echo "🗑️  Removing all mounts and cleaning up temp VM..."
				echo ""
				
				# Show what will be removed
				if command -v yq &> /dev/null; then
					local mount_count=$(yq -r '.mounts | length' "$TEMP_STATE_FILE" 2>/dev/null)
					echo "This will remove $mount_count mount(s) and clean up volumes:"
					yq -r '.mounts[]? | "  📂 \(.source)"' "$TEMP_STATE_FILE" 2>/dev/null
				fi
				
				# Get confirmation
				if [ "$AUTO_YES" != "true" ]; then
					echo ""
					echo "⚠️  This will destroy the temp VM entirely. Continue? (y/N): "
					read -r response
					case "$response" in
						[yY]|[yY][eE][sS])
							;;
						*)
							echo "❌ Operation cancelled"
							exit 1
							;;
					esac
				fi
				
				# Just call destroy since that does a complete cleanup
				echo ""
				echo "🧹 Destroying temp VM and cleaning up all resources..."
				handle_temp_command "destroy"
				exit 0
			fi
			
			# Parse arguments for single path unmount
			local UNMOUNT_PATH="$1"
			local AUTO_YES=""
			shift
			
			# Check for --yes flag
			while [[ $# -gt 0 ]]; do
				case "$1" in
					--yes|-y)
						AUTO_YES="true"
						shift
						;;
					*)
						echo "❌ Unknown option: $1"
						exit 1
						;;
				esac
			done
			
			# Validate directory name for security
			if ! validate_directory_name "$UNMOUNT_PATH"; then
				exit 1
			fi
			
			# Get absolute path if it's a directory
			local abs_path="$UNMOUNT_PATH"
			if [ -d "$UNMOUNT_PATH" ]; then
				abs_path=$(cd "$UNMOUNT_PATH" && pwd)
			fi
			
			# Check if mount exists
			local mount_found=""
			if command -v yq &> /dev/null; then
				if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
					# New format - check if mount exists
					mount_found=$(yq -r '.mounts[] | select(.source == "'"$abs_path"'") | .source' "$TEMP_STATE_FILE" 2>/dev/null)
				fi
			fi
			
			if [ -z "$mount_found" ]; then
				echo "❌ Mount '$abs_path' not found"
				echo ""
				echo "Current mounts:"
				if command -v yq &> /dev/null; then
					yq -r '.mounts[]? | "  • \(.source)"' "$TEMP_STATE_FILE" 2>/dev/null
				fi
				exit 1
			fi
			
			# Check if it's the last mount
			local mount_count=0
			if command -v yq &> /dev/null; then
				mount_count=$(yq -r '.mounts | length' "$TEMP_STATE_FILE" 2>/dev/null)
			fi
			
			if [ "$mount_count" -le 1 ]; then
				echo "❌ Cannot remove the last mount"
				echo "💡 Use 'vm temp destroy' to remove the temp VM entirely"
				exit 1
			fi
			
			# Show warning and get confirmation
			if [ "$AUTO_YES" != "true" ]; then
				echo "⚠️  This will restart the container (takes ~5 seconds)"
				echo "   Your work will be preserved. Continue? (y/N): "
				read -r response
				case "$response" in
					[yY]|[yY][eE][sS])
						;;
					*)
						echo "❌ Operation cancelled"
						exit 1
						;;
				esac
			fi
			
			# Remove mount from state file
			echo "📝 Updating mount configuration..."
			if command -v yq &> /dev/null; then
				# Create temporary file for the update
				local temp_file=$(mktemp)
				setup_temp_file_cleanup "$temp_file"
				cp "$TEMP_STATE_FILE" "$temp_file"
				
				# Remove the mount - SECURE: use yq --arg to prevent shell injection
				yq --arg path "$abs_path" 'del(.mounts[] | select(.source == $path))' \
					"$temp_file" > "$TEMP_STATE_FILE"
			else
				echo "❌ yq is required for unmount operation"
				exit 1
			fi
			
			# Recreate the container
			recreate_temp_vm_with_mounts "$container_name"
			
			echo ""
			echo "📁 Mount removed successfully:"
			echo "   $abs_path"
			exit 0
			;;
		"mounts")
			# List current directory mounts for temp VM
			require_temp_vm || exit 1
			
			echo "📁 Current Mounts:"
			echo "=================="
			
			if command -v yq &> /dev/null; then
				local container_name=$(yq_raw '.container_name' "$TEMP_STATE_FILE")
				echo "Container: $container_name"
				echo ""
				
				# Check if new format exists
				if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
					# New format
					yq -r '.mounts[]? | "  📂 \(.source) → \(.target) (\(.permissions))"' "$TEMP_STATE_FILE" 2>/dev/null
				else
					# Old format
					yq -r '.mounts[]?' "$TEMP_STATE_FILE" 2>/dev/null | while read -r mount; do
						echo "  📂 $mount"
					done
				fi
			else
				# Fallback display
				echo "  (install yq for better formatting)"
				grep "^  - " "$TEMP_STATE_FILE" | sed 's/^  - /  📂 /'
			fi
			
			echo ""
			echo "💡 Use 'vm temp mount <path>' to add more directories"
			echo "💡 Use 'vm temp unmount <path>' to remove a directory"
			echo "💡 Use 'vm temp unmount --all' to remove all mounts and clean up"
			
			exit 0
			;;
		"list")
			# List active temp VM instances
			echo "📋 Active Temp VMs:"
			echo "==================="
			
			# Show active temp VM from state file
			if [ -f "$TEMP_STATE_FILE" ]; then
				if command -v yq &> /dev/null; then
					local container_name=$(yq_raw '.container_name' "$TEMP_STATE_FILE")
					if [ -n "$container_name" ]; then
						local status="stopped"
						if is_temp_vm_running "$container_name"; then
							status="running"
						fi
						echo ""
						echo "🚀 $container_name ($status)"
						echo "   Created: $(yq_raw '.created_at' "$TEMP_STATE_FILE")"
						echo "   Project: $(yq_raw '.project_dir' "$TEMP_STATE_FILE")"
						
						# Show mount count
						local mount_count=0
						if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
							# New format
							mount_count=$(yq -r '.mounts | length' "$TEMP_STATE_FILE" 2>/dev/null)
						else
							# Old format  
							mount_count=$(yq -r '.mounts | length' "$TEMP_STATE_FILE" 2>/dev/null)
						fi
						echo "   Mounts: $mount_count directories"
						
						if [ "$status" = "running" ]; then
							echo "   💡 Use 'vm temp ssh' to connect"
						else
							echo "   💡 Use 'vm temp start' to start"
						fi
					else
						echo ""
						echo "⚠️  State file exists but container name not found"
					fi
				else
					echo ""
					echo "⚠️  State file exists but yq not available to parse it"
				fi
			else
				echo ""
				echo "📭 No active temp VM found"
				echo "💡 Create one with: vm temp ./your-directory"
			fi
			
			# Show all temp VM containers (running and stopped)
			echo ""
			echo "🐋 All Temp VM Containers:"
			echo "-------------------------"
			local all_containers=$(docker_cmd ps -a --filter "name=vmtemp" --format "{{.Names}}\t{{.Status}}\t{{.CreatedAt}}" 2>/dev/null || true)
			if [ -n "$all_containers" ]; then
				echo "$all_containers" | while IFS=$'\t' read -r name status created; do
					if [[ "$status" == *"Up"* ]]; then
						echo "  🟢 $name - $status"
					else
						echo "  🔴 $name - $status"
					fi
				done
			else
				echo "  No temp VM containers found"
			fi
			
			exit 0
			;;
		"start")
			# Start stopped temp VM
			# Get container name
			local container_name
			container_name=$(get_container_name) || exit 1

			# Check if already running
			if is_temp_vm_running "$container_name"; then
				echo "✅ Temp VM is already running"
				exit 0
			fi

			echo "🚀 Starting temp VM..."
			docker_cmd start "$container_name"

			# Wait for ready
			echo "⏳ Waiting for container to be ready..."
			local max_attempts=15
			local attempt=1
			while [ $attempt -le $max_attempts ]; do
				if docker_cmd exec "$container_name" echo "ready" >/dev/null 2>&1; then
					echo "✅ Temp VM started!"
					exit 0
				fi
				sleep 1
				((attempt++))
			done

			echo "❌ Failed to start temp VM"
			exit 1
			;;
		"stop")
			# Stop temp VM
			# Get container name
			local container_name
			container_name=$(get_container_name) || exit 1

			# Check if running
			if ! is_temp_vm_running "$container_name"; then
				echo "⚠️  Temp VM is not running"
				exit 0
			fi

			echo "🛑 Stopping temp VM..."
			docker_cmd stop "$container_name"
			echo "✅ Temp VM stopped"
			exit 0
			;;
		"restart")
			# Restart temp VM
			# Get container name
			local container_name
			container_name=$(get_container_name) || exit 1

			echo "🔄 Restarting temp VM..."
			
			# Stop if running
			if is_temp_vm_running "$container_name"; then
				docker_cmd stop "$container_name" > /dev/null 2>&1
			fi

			# Start
			docker_cmd start "$container_name"

			# Wait for ready
			echo "⏳ Waiting for container to be ready..."
			local max_attempts=15
			local attempt=1
			while [ $attempt -le $max_attempts ]; do
				if docker_cmd exec "$container_name" echo "ready" >/dev/null 2>&1; then
					echo "✅ Temp VM restarted!"
					exit 0
				fi
				sleep 1
				((attempt++))
			done

			echo "❌ Failed to restart temp VM"
			exit 1
			;;
		"logs")
			# View container logs
			# Get container name
			local container_name
			container_name=$(get_container_name) || exit 1

			# Pass through any additional arguments (like -f for follow)
			shift
			docker_cmd logs "$container_name" "$@"
			exit 0
			;;
		"provision")
			# Re-run provisioning
			# Get container name
			local container_name
			container_name=$(get_container_name) || exit 1

			# Check if container is running
			if ! is_temp_vm_running "$container_name"; then
				echo "❌ Temp VM is not running"
				echo "💡 Start it with: vm temp start"
				exit 1
			fi

			echo "🔧 Re-running provisioning..."
			echo "⚠️  Warning: This will reinstall all packages and may take several minutes"
			echo -n "Continue? (y/N): "
			read -r response
			case "$response" in
				[yY]|[yY][eE][sS])
					# Run ansible playbook inside container
					docker_cmd exec "$container_name" bash -c "
						ansible-playbook -i localhost, -c local \\
						/vm-tool/shared/ansible/playbook.yml
					"
					;;
				*)
					echo "❌ Cancelled"
					exit 1
					;;
			esac
			exit 0
			;;
		*)
			# Not a subcommand, treat as mount specification
			;;
	esac
	
	# Clean up any orphaned resources before creating new temp VM
	cleanup_orphaned_temp_resources
	
	# Parse mount arguments
	MOUNT_ARGS=()
	AUTO_DESTROY=""
	
	# Collect mount arguments until we hit a flag
	while [[ $# -gt 0 ]]; do
		case "$1" in
			--auto-destroy)
				AUTO_DESTROY="true"
				shift
				;;
			*)
				# Collect mount arguments
				MOUNT_ARGS+=("$1")
				shift
				;;
		esac
	done
	
	if [ ${#MOUNT_ARGS[@]} -eq 0 ]; then
		echo "❌ Usage: vm temp <mounts> [--auto-destroy]"
		echo "Example: vm temp ./src ./tests ./docs"
		exit 1
	fi
	
	# Check for backward compatibility with comma-separated mounts
	if [ ${#MOUNT_ARGS[@]} -eq 1 ] && [[ "${MOUNT_ARGS[0]}" == *,* ]]; then
		echo "⚠️  Warning: Comma-separated mounts are deprecated"
		echo "   Please use: vm temp ${MOUNT_ARGS[0]//,/ }"
		echo ""
		# Convert comma to array for processing
		old_ifs="$IFS"
		IFS=','
		MOUNT_ARGS=(${MOUNT_ARGS[0]})  # Intentionally unquoted for word splitting
		IFS="$old_ifs"
	fi
	
	# Validate mount directories exist
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: Validating ${#MOUNT_ARGS[@]} mounts" >&2
	fi
	
	# Process and validate each mount
	PROCESSED_MOUNTS=()
	for mount in "${MOUNT_ARGS[@]}"; do
		mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
		original_mount="$mount"
		source=""
		perm="rw"  # default permission
		
		# Parse permission suffix if present
		if [[ "$mount" =~ ^(.+):(ro|rw)$ ]]; then
			source="${BASH_REMATCH[1]}"
			perm="${BASH_REMATCH[2]}"
		elif [[ "$mount" == *:* ]]; then
			# Invalid permission format
			echo "❌ Error: Invalid permission format in '$mount'"
			echo "💡 Valid permissions are :ro (read-only) or :rw (read-write)"
			exit 1
		else
			source="$mount"
		fi
		
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG: Checking mount: '$source' (original: '$original_mount', perm: '$perm')" >&2
			echo "DEBUG: Directory exists: $([ -d "$source" ] && echo "yes" || echo "no")" >&2
			echo "DEBUG: Full path would be: $(realpath "$source" 2>/dev/null || echo "INVALID")" >&2
		fi
		
		if [ ! -d "$source" ]; then
			echo "❌ Error: Directory '$source' does not exist"
			echo "📂 Current directory: $(pwd)"
			echo "💡 Make sure the directory exists or use an absolute path"
			exit 1
		fi
		
		# Store processed mount with permission
		PROCESSED_MOUNTS+=("$source:$perm")
	done
	
	# Check for existing temp VM
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: About to get temp container name" >&2
		if type -t get_temp_container_name >/dev/null 2>&1; then
			echo "DEBUG: Function get_temp_container_name is defined" >&2
		else
			echo "DEBUG: ERROR - Function get_temp_container_name is NOT defined!" >&2
		fi
	fi
	# Get container name without command substitution to avoid issues
	existing_container=""
	if [ -f "$TEMP_STATE_FILE" ]; then
		if command -v yq &> /dev/null; then
			existing_container=$(yq_raw '.container_name // empty' "$TEMP_STATE_FILE")
		else
			# Fallback to grep if yq is not available
			existing_container=$(grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
		fi
	fi
	TEMP_RET=$?
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: get_temp_container_name returned: $TEMP_RET" >&2
		echo "DEBUG: existing_container='$existing_container'" >&2
		echo "DEBUG: Checking if container exists and is running" >&2
		echo "DEBUG: About to check if block condition" >&2
	fi
	if [ -n "$existing_container" ] && is_temp_vm_running "$existing_container"; then
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG: Inside if block - existing container found" >&2
		fi
		# Active temp VM exists - check if mounts match
		existing_mounts=$(get_temp_mounts)
		
		# Normalize the requested mounts for comparison
		normalized_mounts=""
		for mount in "${PROCESSED_MOUNTS[@]}"; do
			source="${mount%:*}"
			perm="${mount##*:}"
			abs_source="$(realpath "$source" 2>/dev/null || echo "$source")"
			if [ -n "$normalized_mounts" ]; then
				normalized_mounts="$normalized_mounts,"
			fi
			normalized_mounts="$normalized_mounts$abs_source:/workspace/$(basename "$source"):$perm"
		done
		
		if compare_mounts "$existing_mounts" "$normalized_mounts"; then
			# Same mounts - just connect
			echo "🔄 Connecting to existing temp VM with matching mounts..."
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
				docker_cmd rm -f "$existing_container" >/dev/null 2>&1
				docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
				rm -f "$TEMP_STATE_FILE"
			fi
			
			exit 0
		else
			# Different mounts - show options to user
			echo "⚠️  Temp VM already exists with different mounts"
			echo ""
			echo "Current mounts:"
			if command -v yq &> /dev/null && [ -f "$TEMP_STATE_FILE" ]; then
				yq -r '.mounts[]?' "$TEMP_STATE_FILE" 2>/dev/null | while read -r mount; do
					# Extract just the source path for readability
					source_path=$(echo "$mount" | cut -d: -f1)
					echo "  • $(basename "$source_path")"
				done
			else
				echo "  • (Unable to display without yq)"
			fi
			echo ""
			echo "Requested mounts:"
			for mount in "${PROCESSED_MOUNTS[@]}"; do
				echo "  • $mount"
			done
			echo ""
			echo "Options:"
			echo "  1. vm temp destroy && vm temp ${MOUNT_ARGS[*]}"
			echo "  2. vm temp ssh (to connect to existing VM)"
			echo ""
			echo "What would you like to do?"
			echo "  1) Connect to existing VM anyway"
			echo "  2) Destroy existing VM and create new one"
			echo "  3) Cancel"
			echo ""
			echo -n "Choose an option (1-3): "
			read -r choice
			
			case "$choice" in
				1)
					echo "🔗 Connecting to existing VM..."
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
					exit 0
					;;
				2)
					echo "🗑️ Destroying existing VM..."
					docker_cmd rm -f "$existing_container" >/dev/null 2>&1
					docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
					rm -f "$TEMP_STATE_FILE"
					# Continue to create new VM
					;;
				*)
					echo "❌ Operation cancelled"
					exit 1
					;;
			esac
		fi
	else
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG: No existing container or not running" >&2
		fi
	fi
	
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: Proceeding to create new temp VM" >&2
	fi
	
	# No existing VM or user chose to recreate - proceed with creation
	TEMP_CONTAINER="vmtemp-dev"  # Use consistent naming with regular VMs
	
	# Source the deep merge utility
	if [ ! -f "$SCRIPT_DIR/shared/deep-merge.sh" ]; then
		echo "❌ Error: Required file not found: $SCRIPT_DIR/shared/deep-merge.sh"
		echo "💡 Make sure the VM tool is properly installed"
		exit 1
	fi
	source "$SCRIPT_DIR/shared/deep-merge.sh"
	
	# Generate minimal temporary vm.yaml config with just overrides
	TEMP_CONFIG_FILE=$(mktemp /tmp/vm-temp.XXXXXX.yaml)
	
	# Setup cleanup function for interruptions
	cleanup_on_interrupt() {
		echo ""
		echo "🛑 Interrupted! Cleaning up..."
		# Remove temp container if it was created
		if [ -n "${TEMP_CONTAINER:-}" ]; then
			docker_cmd rm -f "$TEMP_CONTAINER" >/dev/null 2>&1 || true
		fi
		# Clean up volumes
		docker_cmd volume rm vmtemp_nvm vmtemp_cache vmtemp_config 2>/dev/null || true
		# Clean up network
		docker_cmd network rm vm-temp-project_vmtemp_network 2>/dev/null || true
		# Remove state file
		rm -f "$TEMP_STATE_FILE"
		# Remove temp files
		rm -f "$TEMP_CONFIG_FILE"
		rm -rf "$TEMP_PROJECT_DIR"
		echo "✅ Cleanup complete"
		exit 1
	}
	
	# Ensure cleanup happens on exit or interrupt
	trap 'rm -f "$TEMP_CONFIG_FILE"' EXIT
	trap 'cleanup_on_interrupt' INT TERM
	
	cat > "$TEMP_CONFIG_FILE" <<EOF
project:
  name: vmtemp
  hostname: vm-temp.local
terminal:
  username: vm-temp
  emoji: "🔧"
services:
  postgresql:
    enabled: false
  redis:
    enabled: false
  mongodb:
    enabled: false
EOF
	
	# Extract schema defaults and merge with temp overrides
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: Extracting schema defaults from: $SCRIPT_DIR/vm.schema.yaml" >&2
	fi
	
	SCHEMA_DEFAULTS=$("$SCRIPT_DIR/validate-config.sh" --extract-defaults "$SCRIPT_DIR/vm.schema.yaml" 2>&1)
	if [ $? -ne 0 ]; then
		echo "❌ Failed to extract schema defaults"
		echo "📋 Error output: $SCHEMA_DEFAULTS"
		rm -f "$TEMP_CONFIG_FILE"
		exit 1
	fi
	
	# Use yq to merge schema defaults with temp config
	CONFIG=$(yq -s '.[0] * .[1]' <(echo "$SCHEMA_DEFAULTS") "$TEMP_CONFIG_FILE" 2>&1)
	if [ $? -ne 0 ]; then
		echo "❌ Failed to generate temp VM configuration"
		echo "📋 Error merging configs: $CONFIG"
		echo "💡 Check that yq is installed and working: yq --version"
		rm -f "$TEMP_CONFIG_FILE"
		exit 1
	fi
	
	# Create a temporary project directory for docker-compose generation
	# Use a fixed name instead of PID to avoid multiple projects
	TEMP_PROJECT_DIR="/tmp/vm-temp-project"
	mkdir -p "$TEMP_PROJECT_DIR"
	
	# Track the real paths for direct volume mounts
	MOUNT_MAPPINGS=()
	MOUNT_PERMISSIONS=()
	for mount in "${PROCESSED_MOUNTS[@]}"; do
		source="${mount%:*}"
		perm="${mount##*:}"
		# Get absolute path
		REAL_PATH=$(realpath "$source")
		MOUNT_NAME=$(basename "$source")
		# Store the mapping for Docker volumes (without permission for compatibility)
		MOUNT_MAPPINGS+=("$REAL_PATH:$MOUNT_NAME")
		# Store permissions separately if needed in future
		MOUNT_PERMISSIONS+=("$perm")
	done
	
	# Export mount mappings for docker provisioning script
	export VM_TEMP_MOUNTS="${MOUNT_MAPPINGS[*]}"
	# TODO: In Phase 2, pass mount permissions to docker provisioning
	# export VM_TEMP_MOUNT_PERMISSIONS="${MOUNT_PERMISSIONS[*]}"
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
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: Calling docker_up with:" >&2
		echo "DEBUG:   CONFIG: [truncated for brevity]" >&2
		echo "DEBUG:   TEMP_PROJECT_DIR: $TEMP_PROJECT_DIR" >&2
		echo "DEBUG:   VM_TEMP_MOUNTS: $VM_TEMP_MOUNTS" >&2
	fi
	
	# Call docker_up and capture any errors
	if ! docker_up "$CONFIG" "$TEMP_PROJECT_DIR" "true"; then
		echo "❌ Failed to create temporary VM"
		echo "💡 Tips:"
		echo "   - Run with VM_DEBUG=true for more details"
		echo "   - Check Docker is running: docker ps"
		echo "   - Check disk space: df -h"
		# Clean up on failure
		rm -f "$TEMP_CONFIG_FILE" "$TEMP_DIR_MARKER"
		rm -rf "$TEMP_PROJECT_DIR"
		exit 1
	fi
	
	# Save temp VM state - convert processed mounts back to string for legacy compatibility
	# Will be updated to new format when we update save_temp_state function
	MOUNT_STRING=""
	for mount in "${PROCESSED_MOUNTS[@]}"; do
		if [ -n "$MOUNT_STRING" ]; then
			MOUNT_STRING="$MOUNT_STRING,$mount"
		else
			MOUNT_STRING="$mount"
		fi
	done
	save_temp_state "$TEMP_CONTAINER" "$MOUNT_STRING" "$CURRENT_DIR"
	
	# Clean up temp config file only (keep project dir for mounts)
	rm -f "$TEMP_CONFIG_FILE"
	
	# Handle auto-destroy if flag was set
	if [ "$AUTO_DESTROY" = "true" ]; then
		echo "🗑️ Auto-destroying temp VM..."
		docker_cmd rm -f "$TEMP_CONTAINER" >/dev/null 2>&1
		# Clean up volumes
		docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
		# Remove state file
		rm -f "$TEMP_STATE_FILE"
		# Clean up temp project directory safely
		# Resolve real path to prevent directory traversal
		REAL_TEMP_DIR=$(realpath "$TEMP_PROJECT_DIR" 2>/dev/null)
		if [[ "$REAL_TEMP_DIR" == "/tmp/vm-temp-project" ]]; then
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
}
