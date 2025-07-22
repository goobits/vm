#!/bin/bash
# VM Temporary VM Management Module
# Extracted from vm.sh to improve maintainability

# Temp VM state management constants and functions
TEMP_STATE_DIR="$HOME/.vm"
TEMP_STATE_FILE="$TEMP_STATE_DIR/temp-vm.state"

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
			if [[ "$mount" == *:* ]]; then
				local perm="${mount##*:}"
				local abs_source="$(cd "$project_dir" && realpath "$source" 2>/dev/null || echo "$source")"
				echo "  - $abs_source:/workspace/$(basename "$source"):$perm" >> "$TEMP_STATE_FILE"
			else
				local abs_source="$(cd "$project_dir" && realpath "$source" 2>/dev/null || echo "$source")"
				echo "  - $abs_source:/workspace/$(basename "$source"):rw" >> "$TEMP_STATE_FILE"
			fi
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
		yq . "$TEMP_STATE_FILE" 2>/dev/null
	else
		# Fallback to cat if yq is not available
		cat "$TEMP_STATE_FILE"
	fi
}

# Get temp VM container name from state
get_temp_container_name() {
	echo "DEBUG: Inside get_temp_container_name" >&2
	if [ ! -f "$TEMP_STATE_FILE" ]; then
		echo "DEBUG: State file not found" >&2
		echo ""
		return 1
	fi
	
	if command -v yq &> /dev/null; then
		yq -r '.container_name // empty' "$TEMP_STATE_FILE" 2>/dev/null
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
		yq -r '.mounts[]? // empty' "$TEMP_STATE_FILE" 2>/dev/null | tr '\n' ',' | sed 's/,$//'
	else
		# Fallback to awk if yq is not available
		awk '/^mounts:/{flag=1; next} /^[^ ]/{flag=0} flag && /^  -/{print substr($0, 5)}' "$TEMP_STATE_FILE" 2>/dev/null | tr '\n' ',' | sed 's/,$//'
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
	
	if [ $# -eq 0 ]; then
		echo "❌ Usage: vm temp <command> [options]"
		echo ""
		echo "Commands:"
		echo "  <mounts>              Create/connect to temp VM with specified mounts"
		echo "  ssh                   SSH into the active temp VM"
		echo "  status                Show status of the active temp VM"
		echo "  destroy               Destroy the active temp VM"
		echo ""
		echo "Examples:"
		echo "  vm temp ./client,./server,./shared"
		echo "  vm temp ./src --auto-destroy"
		echo "  vm temp ssh"
		echo "  vm temp status"
		echo "  vm temp destroy"
		exit 1
	fi
	
	# Handle subcommands
	case "$1" in
		"destroy")
			# Destroy temp VM
			if [ ! -f "$TEMP_STATE_FILE" ]; then
				echo "❌ No active temp VM found"
				exit 0
			fi
			
			container_name=$(get_temp_container_name)
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
			if [ ! -f "$TEMP_STATE_FILE" ]; then
				echo "❌ No active temp VM found"
				echo "💡 Create one with: vm temp ./your-directory"
				exit 1
			fi
			
			container_name=$(get_temp_container_name)
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
			if [ ! -f "$TEMP_STATE_FILE" ]; then
				echo "❌ No active temp VM found"
				exit 0
			fi
			
			container_name=$(get_temp_container_name)
			echo "📋 Temp VM Status:"
			echo "=================="
			
			if command -v yq &> /dev/null; then
				echo "Container: $(yq -r '.container_name' "$TEMP_STATE_FILE")"
				echo "Created: $(yq -r '.created_at' "$TEMP_STATE_FILE")"
				echo "Project: $(yq -r '.project_dir' "$TEMP_STATE_FILE")"
				echo ""
				echo "Mounts:"
				yq -r '.mounts[]?' "$TEMP_STATE_FILE" 2>/dev/null | while read -r mount; do
					echo "  • $mount"
				done
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
		*)
			# Not a subcommand, treat as mount specification
			;;
	esac
	
	# Parse mount string from first argument
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
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: Validating mounts: $MOUNT_STRING" >&2
	fi
	
	old_ifs="$IFS"
	IFS=','
	MOUNTS=($MOUNT_STRING)  # This is intentionally unquoted for word splitting
	IFS="$old_ifs"
	
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: Parsed ${#MOUNTS[@]} mounts" >&2
	fi
	
	for mount in "${MOUNTS[@]}"; do
		mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
		original_mount="$mount"
		if [[ "$mount" == *:* ]]; then
			mount="${mount%:*}"
		fi
		
		if [ "${VM_DEBUG:-}" = "true" ]; then
			echo "DEBUG: Checking mount: '$mount' (original: '$original_mount')" >&2
			echo "DEBUG: Directory exists: $([ -d "$mount" ] && echo "yes" || echo "no")" >&2
			echo "DEBUG: Full path would be: $(realpath "$mount" 2>/dev/null || echo "INVALID")" >&2
		fi
		
		if [ ! -d "$mount" ]; then
			echo "❌ Error: Directory '$mount' does not exist"
			echo "📂 Current directory: $(pwd)"
			echo "💡 Make sure the directory exists or use an absolute path"
			exit 1
		fi
	done
	
	# Check for existing temp VM
	if [ "${VM_DEBUG:-}" = "true" ]; then
		echo "DEBUG: About to get temp container name" >&2
	fi
	existing_container=$(get_temp_container_name)
	TEMP_RET=$?
	echo "DEBUG: IMMEDIATE TEST - Assignment complete" >&2
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
		for mount in "${MOUNTS[@]}"; do
			mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
			source="${mount%:*}"
			perm="rw"
			if [[ "$mount" == *:* ]]; then
				perm="${mount##*:}"
			fi
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
			# Different mounts - ask user what to do
			echo "⚠️ Active temp VM exists with different mounts."
			echo ""
			echo "Existing mounts:"
			if command -v yq &> /dev/null && [ -f "$TEMP_STATE_FILE" ]; then
				yq -r '.mounts[]?' "$TEMP_STATE_FILE" 2>/dev/null | while read -r mount; do
					echo "  • $mount"
				done
			else
				echo "  (Unable to display without yq)"
			fi
			echo ""
			echo "Requested mounts:"
			for mount in "${MOUNTS[@]}"; do
				mount=$(echo "$mount" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
				echo "  • $mount"
			done
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
	
	# Save temp VM state
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
}
