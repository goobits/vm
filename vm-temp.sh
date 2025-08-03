#!/bin/bash
# VM Temporary VM Management Module
# Extracted from vm.sh to improve maintainability

# Get the script directory for sourcing shared utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source shared utilities
source "$SCRIPT_DIR/shared/temp-file-utils.sh"
source "$SCRIPT_DIR/shared/security-utils.sh"
source "$SCRIPT_DIR/shared/config-processor.sh"
source "$SCRIPT_DIR/shared/provider-interface.sh"

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

# Directory validation functions moved to shared/security-utils.sh

# Extract container name from state file
# This centralizes the container name retrieval logic to reduce duplication
get_container_name_from_state() {
	local container_name=""
	if command -v yq &> /dev/null; then
		container_name=$(yq_raw '.container_name // empty' "$TEMP_STATE_FILE")
	else
		container_name=$(grep "^container_name:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
	fi
	echo "$container_name"
}

# Get container name from state file with comprehensive error handling
get_container_name() {
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
		echo "❌ No temp VM found" >&2
		echo "💡 Create one with: vm temp ./your-directory" >&2
		return 1
	fi

	# Validate state file integrity
	if ! validate_state_file; then
		return 1
	fi

	local container_name
	container_name=$(get_container_name_from_state)

	if [[ -z "$container_name" ]]; then
		echo "❌ Error: Could not read container name from state file" >&2
		echo "📁 State file: $TEMP_STATE_FILE" >&2
		return 1
	fi

	echo "$container_name"
}

# Validate that temp VM state file exists
require_temp_vm() {
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
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

# Temp file cleanup functions moved to shared/temp-file-utils.sh

# Validate state file is not corrupted
validate_state_file() {
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
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
	local provider="${4:-docker}"  # Default to docker for backwards compatibility
	
	# Create state directory if it doesn't exist
	mkdir -p "$TEMP_STATE_DIR"
	
	# Create state file with YAML format
	cat > "$TEMP_STATE_FILE" <<-EOF
	container_name: $container_name
	provider: $provider
	created_at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
	project_dir: $project_dir
	mounts:
	EOF
	
	# Add mounts to the state file
	if [[ -n "$mounts" ]]; then
		# Split mounts by comma and add to state file
		local old_ifs="$IFS"
		IFS=','
		local MOUNT_ARRAY
		IFS=',' read -ra MOUNT_ARRAY <<< "$mounts"
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
			local abs_source
			abs_source="$(cd "$project_dir" && realpath "$source" 2>/dev/null || echo "$source")"
			local target
			target="/workspace/$(basename "$source")"
			
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
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
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
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG get_temp_container_name: called at $(date)" >&2
	fi
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
		if [[ "${VM_DEBUG:-}" = "true" ]]; then
			echo "DEBUG get_temp_container_name: state file not found" >&2
		fi
		echo ""
		return 1
	fi
	
	get_container_name_from_state
}

# Get provider from temp VM state
get_temp_provider() {
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
		echo "docker"  # Default to docker for backwards compatibility
		return 1
	fi
	
	local provider=""
	if command -v yq &> /dev/null; then
		provider=$(yq_raw '.provider // empty' "$TEMP_STATE_FILE")
	else
		provider=$(grep "^provider:" "$TEMP_STATE_FILE" 2>/dev/null | cut -d: -f2- | sed 's/^[[:space:]]*//')
	fi
	
	# Default to docker if not specified (backwards compatibility)
	if [[ -z "$provider" ]]; then
		provider="docker"
	fi
	
	echo "$provider"
}

# Check if temp VM is running with comprehensive error handling
is_temp_vm_running() {
	local container_name="$1"
	local provider="${2:-}"
	
	if [[ -z "$container_name" ]]; then
		if [[ "${VM_DEBUG:-}" = "true" ]]; then
			echo "DEBUG is_temp_vm_running: empty container name provided" >&2
		fi
		return 1
	fi
	
	# Get provider if not provided
	if [[ -z "$provider" ]]; then
		provider=$(get_temp_provider)
	fi
	
	case "$provider" in
		"docker")
			# Check if container exists and is running
			local container_status
			if ! container_status=$(docker_cmd inspect "$container_name" --format='{{.State.Status}}' 2>/dev/null); then
				if [[ "${VM_DEBUG:-}" = "true" ]]; then
					echo "DEBUG is_temp_vm_running: container '$container_name' not found" >&2
				fi
				return 1
			fi
			
			if [[ "$container_status" = "running" ]]; then
				return 0
			else
				if [[ "${VM_DEBUG:-}" = "true" ]]; then
					echo "DEBUG is_temp_vm_running: container '$container_name' status: $container_status" >&2
				fi
				return 1
			fi
			;;
		"vagrant")
			# Check if Vagrant VM exists and is running
			local vagrant_dir="$SCRIPT_DIR/providers/vagrant"
			local vm_status
			if ! vm_status=$(cd "$vagrant_dir" && vagrant status "$container_name" 2>/dev/null | grep "$container_name" | awk '{print $2}'); then
				if [[ "${VM_DEBUG:-}" = "true" ]]; then
					echo "DEBUG is_temp_vm_running: vagrant VM '$container_name' not found" >&2
				fi
				return 1
			fi
			
			if [[ "$vm_status" = "running" ]]; then
				return 0
			else
				if [[ "${VM_DEBUG:-}" = "true" ]]; then
					echo "DEBUG is_temp_vm_running: vagrant VM '$container_name' status: $vm_status" >&2
				fi
				return 1
			fi
			;;
		*)
			echo "❌ Unsupported provider: $provider" >&2
			return 1
			;;
	esac
}

# Get mounts from state file
get_temp_mounts() {
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
		echo ""
		return 1
	fi
	
	if command -v yq &> /dev/null; then
		# Handle both old and new formats
		# First try old format (simple string)
		local old_format
		old_format=$(yq -r '.mounts[]? // empty' "$TEMP_STATE_FILE" 2>/dev/null | grep -E '^[^:]+:[^:]+:[^:]+$' | tr '\n' ',' | sed 's/,$//')
		if [[ -n "$old_format" ]]; then
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
	local sorted_existing
	sorted_existing=$(echo "$existing" | tr ',' '\n' | sort | tr '\n' ',' | sed 's/,$//')
	local sorted_requested
	sorted_requested=$(echo "$requested" | tr ',' '\n' | sort | tr '\n' ',' | sed 's/,$//')
	
	[ "$sorted_existing" = "$sorted_requested" ]
}

# Clean up orphaned Docker resources from previous temp VM runs
cleanup_orphaned_temp_resources() {
	# Remove orphaned temp VM networks that don't have running containers  
	{
		docker network ls --filter "name=vm-temp-project" -q | while read -r network_id; do
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

#=============================================================================
# VAGRANT TEMP VM FUNCTIONS
# Vagrant-specific implementations for temp VM lifecycle
#=============================================================================

# Create Vagrant temp VM with mounts
vagrant_temp_create() {
	local vm_name="$1"
	local mount_string="$2"
	local project_dir="$3"
	
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG vagrant_temp_create: vm_name='$vm_name', mount_string='$mount_string', project_dir='$project_dir'" >&2
	fi
	
	# Create temporary directory for Vagrant configuration
	local temp_vagrant_dir
	temp_vagrant_dir=$(mktemp -d "/tmp/vagrant-temp-${vm_name}.XXXXXX")
	setup_temp_file_cleanup "$temp_vagrant_dir"
	
	# Parse mount string into array
	local mount_array=()
	if [[ -n "$mount_string" ]]; then
		local old_ifs="$IFS"
		IFS=','
		IFS=',' read -ra mount_array <<< "$mount_string"
		IFS="$old_ifs"
	fi
	
	# Generate Vagrantfile using Ruby config generator
	local config_generator="$SCRIPT_DIR/providers/vagrant/vagrant-temp-config.rb"
	if [[ ! -f "$config_generator" ]]; then
		echo "❌ Vagrant temp config generator not found: $config_generator" >&2
		return 1
	fi
	
	# Generate Vagrantfile
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG vagrant_temp_create: generating Vagrantfile with: ruby '$config_generator' '$vm_name' ${mount_array[*]}" >&2
	fi
	
	if ! ruby "$config_generator" "$vm_name" "${mount_array[@]}" > "$temp_vagrant_dir/Vagrantfile"; then
		echo "❌ Failed to generate Vagrantfile for temp VM" >&2
		return 1
	fi
	
	# Start the Vagrant VM
	echo "🚀 Creating Vagrant temp VM: $vm_name"
	cd "$temp_vagrant_dir"
	
	export VM_PROJECT_DIR="$project_dir"
	
	if ! vagrant up "$vm_name" 2>&1; then
		echo "❌ Failed to create Vagrant temp VM" >&2
		return 1
	fi
	
	# Wait for VM to be ready
	echo "⏳ Waiting for VM to be ready..."
	local max_attempts=30
	local attempt=1
	
	while [[ $attempt -le $max_attempts ]]; do
		if vagrant ssh "$vm_name" -c "test -f /tmp/provisioning_complete" >/dev/null 2>&1; then
			echo "✅ Vagrant temp VM ready!"
			return 0
		fi
		
		echo "⏳ Provisioning in progress... ($attempt/$max_attempts)"
		sleep 2
		((attempt++))
	done
	
	echo "❌ Vagrant temp VM creation timed out" >&2
	return 1
}

# SSH into Vagrant temp VM
vagrant_temp_ssh() {
	local vm_name="$1"
	
	# Find the temp Vagrant directory
	local vagrant_dir
	vagrant_dir=$(find /tmp -maxdepth 1 -type d -name "vagrant-temp-${vm_name}.*" 2>/dev/null | head -1)
	
	if [[ -z "$vagrant_dir" ]]; then
		echo "❌ Vagrant temp VM directory not found for $vm_name" >&2
		echo "💡 The VM may have been destroyed or cleaned up" >&2
		return 1
	fi
	
	echo "🔗 Connecting to Vagrant temp VM: $vm_name"
	cd "$vagrant_dir"
	vagrant ssh "$vm_name" "$@"
}

# Destroy Vagrant temp VM
vagrant_temp_destroy() {
	local vm_name="$1"
	
	# Find the temp Vagrant directory
	local vagrant_dir
	vagrant_dir=$(find /tmp -maxdepth 1 -type d -name "vagrant-temp-${vm_name}.*" 2>/dev/null | head -1)
	
	if [[ -n "$vagrant_dir" ]] && [[ -d "$vagrant_dir" ]]; then
		echo "🗑️ Destroying Vagrant temp VM: $vm_name"
		cd "$vagrant_dir"
		vagrant destroy -f "$vm_name" >/dev/null 2>&1 || true
		
		# Clean up temp directory
		cd /tmp
		rm -rf "$vagrant_dir" 2>/dev/null || true
		echo "✅ Vagrant temp VM destroyed"
	else
		echo "⚠️ Vagrant temp VM directory not found, may already be destroyed"
	fi
}

# Get Vagrant temp VM status
vagrant_temp_status() {
	local vm_name="$1"
	
	# Find the temp Vagrant directory
	local vagrant_dir
	vagrant_dir=$(find /tmp -maxdepth 1 -type d -name "vagrant-temp-${vm_name}.*" 2>/dev/null | head -1)
	
	if [[ -z "$vagrant_dir" ]]; then
		echo "not found"
		return 1
	fi
	
	cd "$vagrant_dir"
	local status
	status=$(vagrant status "$vm_name" 2>/dev/null | grep "$vm_name" | awk '{print $2}' || echo "unknown")
	echo "$status"
}

# Start Vagrant temp VM
vagrant_temp_start() {
	local vm_name="$1"
	
	# Find the temp Vagrant directory
	local vagrant_dir
	vagrant_dir=$(find /tmp -maxdepth 1 -type d -name "vagrant-temp-${vm_name}.*" 2>/dev/null | head -1)
	
	if [[ -z "$vagrant_dir" ]]; then
		echo "❌ Vagrant temp VM directory not found for $vm_name" >&2
		return 1
	fi
	
	echo "🚀 Starting Vagrant temp VM: $vm_name"
	cd "$vagrant_dir"
	vagrant up "$vm_name"
}

# Stop Vagrant temp VM
vagrant_temp_stop() {
	local vm_name="$1"
	
	# Find the temp Vagrant directory
	local vagrant_dir
	vagrant_dir=$(find /tmp -maxdepth 1 -type d -name "vagrant-temp-${vm_name}.*" 2>/dev/null | head -1)
	
	if [[ -z "$vagrant_dir" ]]; then
		echo "❌ Vagrant temp VM directory not found for $vm_name" >&2
		return 1
	fi
	
	echo "🛑 Stopping Vagrant temp VM: $vm_name"
	cd "$vagrant_dir"
	vagrant halt "$vm_name"
}

# Update Vagrant temp VM with new mounts (requires recreation)
vagrant_temp_update_mounts() {
	local vm_name="$1"
	local new_mount_string="$2"
	local project_dir="$3"
	
	echo "🔄 Updating Vagrant temp VM mounts (requires recreation)..."
	echo "⚠️ This will restart the VM to apply new mount configuration"
	
	# Destroy and recreate with new mounts
	vagrant_temp_destroy "$vm_name"
	vagrant_temp_create "$vm_name" "$new_mount_string" "$project_dir"
}

#=============================================================================
# DOCKER TEMP VM FUNCTIONS (EXISTING)
#=============================================================================

# Update temp VM with new mounts (preserves container) with comprehensive error recovery
update_temp_vm_with_mounts() {
	local container_name="$1"
	local start_time
	start_time=$(date +%s)
	
	# Validate input
	if [[ -z "$container_name" ]]; then
		echo "❌ Error: Container name is required" >&2
		return 1
	fi
	
	# Validate state file exists and is readable
	if [[ ! -f "$TEMP_STATE_FILE" ]]; then
		echo "❌ Error: Temp VM state file not found: $TEMP_STATE_FILE" >&2
		return 1
	fi
	
	if ! validate_state_file; then
		echo "❌ Error: Temp VM state file is corrupted" >&2
		return 1
	fi
	
	echo "🔄 Updating container with new mounts..."
	
	# Save current container state for rollback
	local container_backup_state
	container_backup_state=$(docker_cmd inspect "$container_name" --format='{{.State.Status}}' 2>/dev/null || echo "unknown")
	
	# Get current state
	local project_dir
	project_dir=$(yq_raw '.project_dir' "$TEMP_STATE_FILE")
	if [[ -z "$project_dir" ]]; then
		project_dir=""
	fi
	if [[ -z "$project_dir" ]]; then
		echo "❌ Error: Could not read project directory from state file" >&2
		echo "📁 State file: $TEMP_STATE_FILE" >&2
		return 1
	fi
	
	# Validate project directory exists
	if [[ ! -d "$project_dir" ]]; then
		echo "❌ Error: Project directory from state file does not exist: $project_dir" >&2
		return 1
	fi
	
	# Read current mount configuration and build mount string for docker provisioning
	local mount_string=""
	if command -v yq &> /dev/null; then
		# Check format and build mount string (space-separated realpath:basename pairs)
		if yq -r '.mounts[0] | has("source")' "$TEMP_STATE_FILE" 2>/dev/null | grep -q "true"; then
			# New format - extract source paths and create realpath:basename format
			mount_string=$(yq -r '.mounts[] | .source' "$TEMP_STATE_FILE" 2>/dev/null | while read -r source; do
				echo -n "$(realpath "$source" 2>/dev/null || echo "$source"):$(basename "$source") "
			done | sed 's/ $//')
		else
			# Old format - parse and convert to expected format
			mount_string=$(yq -r '.mounts[]' "$TEMP_STATE_FILE" 2>/dev/null | while read -r mount; do
				# Extract source from old format mount string
				source="${mount%%:*}"
				echo -n "$(realpath "$source" 2>/dev/null || echo "$source"):$(basename "$source") "
			done | sed 's/ $//')
		fi
	fi
	
	echo "🛑 Stopping container..."
	local stop_success=true
	if ! docker_cmd stop "$container_name" > /dev/null 2>&1; then
		echo "⚠️ Warning: Failed to stop container gracefully, trying force stop..." >&2
		if ! docker_cmd kill "$container_name" > /dev/null 2>&1; then
			echo "❌ Error: Failed to stop container '$container_name'" >&2
			echo "💡 Container may be unresponsive. Try: vm temp destroy" >&2
			stop_success=false
		else
			echo "⚠️ Container force stopped" >&2
		fi
	fi
	
	# Set environment variables for temp VM
	export VM_TEMP_MOUNTS="$mount_string"
	export VM_IS_TEMP="true"
	
	# Create the container configuration in YAML format
	local config="version: '1.0'
project:
  name: vm-temp-project
  type: single
settings:
  ssh_port: 2222
  providers: docker
environments:
  dev:
    type: ubuntu
    providers:
      docker:
        image: vm-ubuntu-24.04:latest
        container_name: $container_name
        init_script: /opt/provision.sh
        # Security: Removed privileged mode - creates container escape risks
        # Instead use minimal capabilities for development workflows:
        cap_add:
          - CHOWN        # Change file ownership (needed for development file operations)
          - SETUID       # Set user ID (needed for sudo and user switching)  
          - SETGID       # Set group ID (needed for proper group permissions)
        # privileged: true
        network_mode: bridge
        volumes:
          - type: volume
            source: vmtemp_home
            target: /home/developer"
	
	echo "🔄 Updating container configuration..."
	
	# Create temporary config file with .yaml extension
	local temp_config_file
	temp_config_file=$(mktemp /tmp/vm-temp-config.XXXXXX.yaml)
	setup_temp_file_cleanup "$temp_config_file"
	echo "$config" > "$temp_config_file"
	
	# Generate docker-compose.yml
	"$SCRIPT_DIR/providers/docker/docker-provisioning-simple.sh" "$temp_config_file" "$project_dir"
	
	# Temp file will be cleaned up automatically by trap handler
	
	# Apply the new configuration using docker-compose with error recovery
	echo "🚀 Applying new mount configuration..."
	local compose_success=false
	local compose_attempts=0
	local max_compose_attempts=2
	
	while [[ $compose_attempts -lt $max_compose_attempts ]] && [[ $compose_success == false ]]; do
		((compose_attempts++))
		
		if [[ $compose_attempts -eq 1 ]]; then
			# First attempt: try with --no-recreate
			if (cd "$project_dir" && docker-compose up -d --no-recreate 2>/dev/null); then
				compose_success=true
			fi
		else
			# Second attempt: full recreate
			echo "🔄 Retrying with full container recreation..." >&2
			if (cd "$project_dir" && docker-compose up -d 2>&1); then
				compose_success=true
			fi
		fi
	done
	
	if [[ $compose_success == false ]]; then
		echo "❌ Error: Failed to apply new mount configuration after $compose_attempts attempts" >&2
		echo "🧩 Attempting to restore container to previous state..." >&2
		
		# Try to restart the original container
		if [[ "$container_backup_state" == "running" ]]; then
			if docker_cmd start "$container_name" >/dev/null 2>&1; then
				echo "⚠️ Container restored to previous running state" >&2
				echo "💡 Mount update failed but container is accessible" >&2
				return 1
			fi
		fi
		
		echo "❌ Error: Failed to restore container. Manual intervention may be required" >&2
		echo "💡 Try: vm temp destroy && vm temp <your-mounts>" >&2
		return 1
	fi
	
	# Wait for container to be ready with timeout and health checks
	echo "⏳ Waiting for container to be ready..."
	local max_attempts=30
	local attempt=1
	local container_ready=false
	
	while [[ $attempt -le $max_attempts ]]; do
		# First check if container is still running
		if ! is_temp_vm_running "$container_name"; then
			echo "❌ Error: Container stopped unexpectedly during update" >&2
			echo "💡 Check logs: vm temp logs" >&2
			break
		fi
		
		# Check if provisioning is complete
		if docker_cmd exec "$container_name" test -f /tmp/provisioning_complete 2>/dev/null; then
			container_ready=true
			break
		fi
		
		# Also check if container is responding to basic commands
		if docker_cmd exec "$container_name" echo "health-check" >/dev/null 2>&1; then
			if [[ "${VM_DEBUG:-}" = "true" ]]; then
				echo "DEBUG: Container responding but provisioning not complete (attempt $attempt/$max_attempts)" >&2
			fi
		else
			echo "⚠️ Container not responding to exec commands (attempt $attempt/$max_attempts)" >&2
		fi
		
		sleep 1
		((attempt++))
	done
	
	if [[ $container_ready == false ]]; then
		echo "❌ Error: Container did not become ready within timeout" >&2
		echo "💡 Container may be unhealthy. Check logs: vm temp logs" >&2
		return 1
	fi
	
	local end_time
	end_time=$(date +%s)
	local elapsed=$((end_time - start_time))
	
	echo "✅ Container updated with new mounts in ${elapsed} seconds"
	return 0
}

# Handle temp VM commands
handle_temp_command() {
	
	# Add debug output for temp command
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG handle_temp_command: called with args: $*" >&2
		echo "DEBUG handle_temp_command: current directory: $(pwd)" >&2
		echo "DEBUG handle_temp_command: SCRIPT_DIR: $SCRIPT_DIR" >&2
	fi
	
	# Detect the provider to use for temp VMs
	# Priority: 1) Provider from existing temp VM state, 2) PROVIDER env var, 3) Default config scan
	local temp_provider="docker"  # Default to docker
	
	# First check if there's an existing temp VM with a provider set
	if [[ -f "$TEMP_STATE_FILE" ]]; then
		temp_provider=$(get_temp_provider)
		if [[ "${VM_DEBUG:-}" = "true" ]]; then
			echo "DEBUG handle_temp_command: using provider from existing temp VM state: $temp_provider" >&2
		fi
	else
		# Check for explicit provider environment variable
		if [[ -n "${PROVIDER:-}" ]]; then
			temp_provider="$PROVIDER"
			if [[ "${VM_DEBUG:-}" = "true" ]]; then
				echo "DEBUG handle_temp_command: using provider from PROVIDER env var: $temp_provider" >&2
			fi
		else
			# Try to detect from current directory config (scan mode)
			local current_config
			if current_config=$(load_and_merge_config "__SCAN__" "$(pwd)" 2>/dev/null); then
				temp_provider=$(get_config_provider "$current_config")
				if [[ "${VM_DEBUG:-}" = "true" ]]; then
					echo "DEBUG handle_temp_command: detected provider from config scan: $temp_provider" >&2
				fi
			fi
		fi
	fi
	
	# Validate and check provider availability
	if ! validate_provider "$temp_provider"; then
		exit 1
	fi
	
	if ! is_provider_available "$temp_provider"; then
		echo "❌ Error: Provider '$temp_provider' is not available on this system" >&2
		case "$temp_provider" in
			"docker")
				echo "💡 Install Docker to use temp VMs: https://docs.docker.com/get-docker/" >&2
				;;
			"vagrant")
				echo "💡 Install Vagrant to use temp VMs: https://www.vagrantup.com/downloads" >&2
				;;
		esac
		exit 1
	fi
	
	# Provider-specific health checks
	case "$temp_provider" in
		"docker")
			# Check if Docker daemon is running
			if ! docker_cmd version >/dev/null 2>&1; then
				echo "❌ Docker daemon is not running or not accessible"
				echo "💡 Tips:"
				echo "   - Start Docker Desktop (if on macOS/Windows)"
				echo "   - Run: sudo systemctl start docker (if on Linux)"
				echo "   - Check permissions: groups | grep docker"
				exit 1
			fi
			;;
		"vagrant")
			# Check if Vagrant is working properly
			if ! vagrant version >/dev/null 2>&1; then
				echo "❌ Vagrant is not working properly"
				echo "💡 Try: vagrant --version"
				exit 1
			fi
			;;
	esac
	
	# Export provider for use by other functions
	export TEMP_VM_PROVIDER="$temp_provider"
	
	# Show help if no arguments or --help flag
	if [[ $# -eq 0 ]] || [[ "$1" = "--help" ]] || [[ "$1" = "-h" ]]; then
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
	
	# Check for running processes in the container
	check_running_processes() {
		local container_name="$1"
		
		# Get process list with detailed information
		local process_output
		if ! process_output=$(docker_cmd exec "$container_name" ps -eo pid,cmd --no-headers 2>/dev/null); then
			echo "Unable to check processes"
			return 1
		fi
		
		# Filter out standard container processes
		local filtered_processes
		filtered_processes=$(echo "$process_output" | grep -v -E '(^[[:space:]]*1[[:space:]]+/(bin/|usr/bin/)?bash|^[[:space:]]*[0-9]+[[:space:]]+/usr/sbin/sshd|^[[:space:]]*[0-9]+[[:space:]]+sshd:|^[[:space:]]*[0-9]+[[:space:]]+/opt/provision\.sh|^[[:space:]]*[0-9]+[[:space:]]+ps -eo)' || true)
		
		if [[ -n "$filtered_processes" ]]; then
			echo "$filtered_processes"
			return 0
		else
			echo ""
			return 1
		fi
	}
	
	# Confirm destruction with user-friendly output
	confirm_destroy() {
		local container_name="$1"
		local force_flag="$2"
		
		echo "🔍 Checking temp VM status..."
		
		# Check if container is running
		local container_status="Stopped"
		local uptime_info=""
		
		if is_temp_vm_running "$container_name"; then
			container_status="Running"
			
			# Get container uptime
			local started_at
			if started_at=$(docker_cmd inspect "$container_name" --format='{{.State.StartedAt}}' 2>/dev/null); then
				# Convert to seconds since epoch for calculation
				local start_epoch current_epoch
				if start_epoch=$(date -d "$started_at" +%s 2>/dev/null) && current_epoch=$(date +%s); then
					local uptime_seconds=$((current_epoch - start_epoch))
					local hours=$((uptime_seconds / 3600))
					local minutes=$(((uptime_seconds % 3600) / 60))
					
					if [[ $hours -gt 0 ]]; then
						uptime_info=", ${hours}h ${minutes}m uptime"
					else
						uptime_info=", ${minutes}m uptime"
					fi
				fi
			fi
		fi
		
		echo "📦 Container: $container_name ($container_status$uptime_info)"
		
		# Check for active processes if running
		if [[ "$container_status" = "Running" ]]; then
			local processes
			if processes=$(check_running_processes "$container_name"); then
				echo "🏃 Active processes:"
				echo "$processes" | while IFS= read -r line; do
					local pid cmd
					read -r pid cmd <<< "$line"
					echo "   • $cmd (PID $pid)"
				done
			fi
		fi
		
		echo "🗑️  Will be deleted:"
		echo "   ✗ Container and its filesystem"
		echo "   ✗ Cached data (node_modules, npm cache, etc.)"
		echo "✅ Your project files will remain safe on the host machine."
		
		if [[ "$container_status" = "Running" ]]; then
			echo "⚠️  WARNING: Running processes will be terminated."
		fi
		
		# Skip prompt if force flag is true
		if [[ "$force_flag" = "true" ]]; then
			return 0
		fi
		
		echo -n "Destroy temp VM? (y/N): "
		read -r response
		case "$response" in
			[yY]|[yY][eE][sS])
				return 0
				;;
			*)
				return 1
				;;
		esac
	}
	
	# Handle subcommands
	case "$1" in
		"destroy")
			# Destroy temp VM
			shift
			
			# Parse flags
			local FORCE_DESTROY=""
			while [[ $# -gt 0 ]]; do
				case "$1" in
					--force|-f)
						FORCE_DESTROY="true"
						shift
						;;
					*)
						echo "❌ Unknown option: $1"
						exit 1
						;;
				esac
			done
			
			require_temp_vm || exit 0
			
			# Get container name and provider
			container_name=$(get_container_name_from_state)
			local provider
			provider=$(get_temp_provider)
			
			# Show confirmation dialog and get user consent
			if ! confirm_destroy "$container_name" "$FORCE_DESTROY"; then
				echo "❌ Operation cancelled"
				exit 0
			fi
			
			echo ""
			echo "🗑️ Destroying temp VM: $container_name (provider: $provider)"
			
			# Provider-specific destroy logic
			case "$provider" in
				"docker")
					# Stop container first if running
					if is_temp_vm_running "$container_name" "$provider"; then
						echo "🛑 Stopping container..."
						if ! docker_cmd stop "$container_name" >/dev/null 2>&1; then
							echo "⚠️  Failed to stop container gracefully, forcing..."
							docker_cmd kill "$container_name" >/dev/null 2>&1 || true
						fi
					fi
					
					# Remove container
					if ! docker_cmd rm -f "$container_name" >/dev/null 2>&1; then
						echo "⚠️  Failed to remove container '$container_name'"
						echo "💡 Container may already be removed or you may need sudo access"
					fi
					
					# Clean up volumes (non-critical, continue on failure)
					docker_cmd volume rm vmtemp_nvm vmtemp_cache >/dev/null 2>&1 || true
					;;
				"vagrant")
					# Use Vagrant-specific destroy function
					vagrant_temp_destroy "$container_name"
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			
			# Remove state file
			if ! rm -f "$TEMP_STATE_FILE"; then
				echo "⚠️  Failed to remove state file: $TEMP_STATE_FILE"
			fi
			
			echo "✅ Temp VM destroyed"
			exit 0
			;;
		"ssh")
			shift
			# SSH into temp VM
			require_temp_vm || exit 1
			
			# Get container name and provider
			container_name=$(get_container_name_from_state)
			local provider
			provider=$(get_temp_provider)
			
			if ! is_temp_vm_running "$container_name" "$provider"; then
				echo "❌ Temp VM exists but is not running"
				echo "💡 Check status with: vm temp status"
				exit 1
			fi
			
			echo "🔗 Connecting to $container_name (provider: $provider)..."
			
			# Provider-specific SSH logic
			case "$provider" in
				"docker")
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
					;;
				"vagrant")
					# Use Vagrant-specific SSH function
					vagrant_temp_ssh "$container_name" "$@"
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			exit 0
			;;
		"status")
			# Show temp VM status
			require_temp_vm || exit 0
			
			# Get container name and provider
			container_name=$(get_container_name_from_state)
			local provider
			provider=$(get_temp_provider)
			
			echo "📋 Temp VM Status:"
			echo "=================="
			
			if command -v yq &> /dev/null; then
				echo "Container: $(get_container_name_from_state)"
				echo "Provider: $provider"
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
			
			# Check if VM is actually running (provider-specific)
			if is_temp_vm_running "$container_name" "$provider"; then
				echo ""
				echo "Status: ✅ Running"
			else
				echo ""
				echo "Status: ❌ Not running (state file exists but VM is gone)"
			fi
			exit 0
			;;
		"mount")
			# Add a new mount to running temp VM
			shift
			if [[ $# -eq 0 ]]; then
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
				while [[ $attempt -le $max_attempts ]]; do
					if docker_cmd exec "$container_name" echo "ready" >/dev/null 2>&1; then
						echo "✅ Temp VM started! Now adding mount..."
						break
					fi
					sleep 1
					((attempt++))
				done
				
				if [[ $attempt -gt $max_attempts ]]; then
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
			if [[ ! -d "$source" ]]; then
				echo "❌ Error: Directory '$source' does not exist"
				exit 1
			fi
			
			# Validate directory name for security
			if ! validate_directory_name "$source"; then
				exit 1
			fi
			
			# Get absolute path
			local abs_source
			abs_source=$(cd "$source" && pwd)
			local target
			target="/workspace/$(basename "$abs_source")"
			
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
			if [[ "$AUTO_YES" != "true" ]]; then
				echo "⚠️  This will restart the container (takes ~5 seconds)"
				echo "   Your work will be preserved. Continue? (y/N): "
				read -rr response
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
				local temp_file
				temp_file=$(mktemp)
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
			
			# Update the VM with new mounts (provider-specific)
			local provider
			provider=$(get_temp_provider)
			
			case "$provider" in
				"docker")
					update_temp_vm_with_mounts "$container_name"
					;;
				"vagrant")
					# Get current mount string and add new mount
					local current_mounts
					current_mounts=$(get_temp_mounts)
					local new_mount_string="$current_mounts,$abs_source:$perm"
					if [[ -z "$current_mounts" ]]; then
						new_mount_string="$abs_source:$perm"
					fi
					
					# Get project dir from state
					local project_dir
					project_dir=$(yq_raw '.project_dir' "$TEMP_STATE_FILE")
					
					vagrant_temp_update_mounts "$container_name" "$new_mount_string" "$project_dir"
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			
			echo ""
			echo "📁 Mount added successfully:"
			echo "   $abs_source → $target ($perm)"
			exit 0
			;;
		"unmount")
			# Remove a mount from running temp VM
			shift
			if [[ $# -eq 0 ]]; then
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
			if [[ "$1" = "--all" ]]; then
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
					local mount_count
				mount_count=$(yq -r '.mounts | length' "$TEMP_STATE_FILE" 2>/dev/null)
					echo "This will remove $mount_count mount(s) and clean up volumes:"
					yq -r '.mounts[]? | "  📂 \(.source)"' "$TEMP_STATE_FILE" 2>/dev/null
				fi
				
				# Get confirmation
				if [[ "$AUTO_YES" != "true" ]]; then
					echo ""
					echo "⚠️  This will destroy the temp VM entirely. Continue? (y/N): "
					read -rr response
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
			local abs_path
			abs_path="$UNMOUNT_PATH"
			if [[ -d "$UNMOUNT_PATH" ]]; then
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
			
			if [[ -z "$mount_found" ]]; then
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
			
			if [[ "$mount_count" -le 1 ]]; then
				echo "❌ Cannot remove the last mount"
				echo "💡 Use 'vm temp destroy' to remove the temp VM entirely"
				exit 1
			fi
			
			# Show warning and get confirmation
			if [[ "$AUTO_YES" != "true" ]]; then
				echo "⚠️  This will restart the container (takes ~5 seconds)"
				echo "   Your work will be preserved. Continue? (y/N): "
				read -rr response
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
				local temp_file
				temp_file=$(mktemp)
				setup_temp_file_cleanup "$temp_file"
				cp "$TEMP_STATE_FILE" "$temp_file"
				
				# Remove the mount - SECURE: use yq --arg to prevent shell injection
				yq --arg path "$abs_path" 'del(.mounts[] | select(.source == $path))' \
					"$temp_file" > "$TEMP_STATE_FILE"
			else
				echo "❌ yq is required for unmount operation"
				exit 1
			fi
			
			# Update the VM with new mounts (provider-specific)
			local provider
			provider=$(get_temp_provider)
			
			case "$provider" in
				"docker")
					update_temp_vm_with_mounts "$container_name"
					;;
				"vagrant")
					# Get updated mount string without the removed mount
					local updated_mounts
					updated_mounts=$(get_temp_mounts)
					
					# Get project dir from state
					local project_dir
					project_dir=$(yq_raw '.project_dir' "$TEMP_STATE_FILE")
					
					vagrant_temp_update_mounts "$container_name" "$updated_mounts" "$project_dir"
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			
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
				local container_name
				container_name=$(get_container_name_from_state)
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
			if [[ -f "$TEMP_STATE_FILE" ]]; then
				if command -v yq &> /dev/null; then
					local container_name
				container_name=$(get_container_name_from_state)
					if [[ -n "$container_name" ]]; then
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
						
						if [[ "$status" = "running" ]]; then
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
			local all_containers
			all_containers=$(docker_cmd ps -a --filter "name=vmtemp" --format "{{.Names}}\t{{.Status}}\t{{.CreatedAt}}" 2>/dev/null || true)
			if [[ -n "$all_containers" ]]; then
				echo "$all_containers" | while IFS=$'\t' read -rr name status _; do
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
			# Get container name and provider
			local container_name
			container_name=$(get_container_name) || exit 1
			local provider
			provider=$(get_temp_provider)

			# Check if already running
			if is_temp_vm_running "$container_name" "$provider"; then
				echo "✅ Temp VM is already running"
				exit 0
			fi

			echo "🚀 Starting temp VM (provider: $provider)..."
			
			case "$provider" in
				"docker")
					if ! docker_cmd start "$container_name"; then
						echo "❌ Failed to start container '$container_name'"
						echo "💡 Container may be corrupted. Try: vm temp destroy && vm temp <mounts>"
						exit 1
					fi

					# Wait for ready
					echo "⏳ Waiting for container to be ready..."
					local max_attempts=15
					local attempt=1
					while [[ $attempt -le $max_attempts ]]; do
						# Check if container is still running
						if ! is_temp_vm_running "$container_name" "$provider"; then
							echo "❌ Container stopped unexpectedly during startup"
							echo "💡 Check logs: vm temp logs"
							exit 1
						fi
						
						if docker_cmd exec "$container_name" echo "ready" >/dev/null 2>&1; then
							echo "✅ Temp VM started!"
							exit 0
						fi
						sleep 1
						((attempt++))
					done

					echo "❌ Failed to start temp VM - container not responding"
					echo "💡 Check logs: vm temp logs"
					exit 1
					;;
				"vagrant")
					vagrant_temp_start "$container_name"
					echo "✅ Vagrant temp VM started!"
					exit 0
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			;;
		"stop")
			# Stop temp VM
			# Get container name and provider
			local container_name
			container_name=$(get_container_name) || exit 1
			local provider
			provider=$(get_temp_provider)

			# Check if running
			if ! is_temp_vm_running "$container_name" "$provider"; then
				echo "⚠️  Temp VM is not running"
				exit 0
			fi

			echo "🛑 Stopping temp VM (provider: $provider)..."
			
			case "$provider" in
				"docker")
					if ! docker_cmd stop "$container_name"; then
						echo "⚠️  Failed to stop container gracefully, trying force stop..."
						if ! docker_cmd kill "$container_name" 2>/dev/null; then
							echo "❌ Failed to stop container"
							echo "💡 Container may already be stopped or you may need sudo access"
							exit 1
						fi
						echo "⚠️  Container force stopped"
					else
						echo "✅ Temp VM stopped"
					fi
					;;
				"vagrant")
					vagrant_temp_stop "$container_name"
					echo "✅ Vagrant temp VM stopped"
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			exit 0
			;;
		"restart")
			# Restart temp VM
			# Get container name and provider
			local container_name
			container_name=$(get_container_name) || exit 1
			local provider
			provider=$(get_temp_provider)

			echo "🔄 Restarting temp VM (provider: $provider)..."
			
			case "$provider" in
				"docker")
					# Stop if running
					if is_temp_vm_running "$container_name" "$provider"; then
						docker_cmd stop "$container_name" > /dev/null 2>&1
					fi

					# Start
					docker_cmd start "$container_name"

					# Wait for ready
					echo "⏳ Waiting for container to be ready..."
					local max_attempts=15
					local attempt=1
					while [[ $attempt -le $max_attempts ]]; do
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
				"vagrant")
					# For Vagrant, we restart by halting then bringing back up
					vagrant_temp_stop "$container_name"
					vagrant_temp_start "$container_name"
					echo "✅ Vagrant temp VM restarted!"
					exit 0
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			;;
		"logs")
			# View VM logs
			# Get container name and provider
			local container_name
			container_name=$(get_container_name) || exit 1
			local provider
			provider=$(get_temp_provider)

			# Pass through any additional arguments (like -f for follow)
			shift
			
			case "$provider" in
				"docker")
					docker_cmd logs "$container_name" "$@"
					;;
				"vagrant")
					echo "📋 Viewing Vagrant VM logs..."
					echo "💡 For detailed logs, you can SSH into the VM and use journalctl"
					
					# Find the temp Vagrant directory
					local vagrant_dir
					vagrant_dir=$(find /tmp -maxdepth 1 -type d -name "vagrant-temp-${container_name}.*" 2>/dev/null | head -1)
					
					if [[ -n "$vagrant_dir" ]] && [[ -d "$vagrant_dir" ]]; then
						cd "$vagrant_dir"
						vagrant ssh "$container_name" -c "sudo journalctl -f" 2>/dev/null || echo "⚠️ Unable to access VM logs"
					else
						echo "❌ Vagrant temp VM directory not found"
					fi
					;;
				*)
					echo "❌ Unsupported provider: $provider" >&2
					exit 1
					;;
			esac
			exit 0
			;;
		"provision")
			# Re-run provisioning
			# Get container name and provider
			local container_name
			container_name=$(get_container_name) || exit 1
			local provider
			provider=$(get_temp_provider)

			# Check if VM is running
			if ! is_temp_vm_running "$container_name" "$provider"; then
				echo "❌ Temp VM is not running"
				echo "💡 Start it with: vm temp start"
				exit 1
			fi

			echo "🔧 Re-running provisioning (provider: $provider)..."
			echo "⚠️  Warning: This will reinstall all packages and may take several minutes"
			echo -n "Continue? (y/N): "
			read -r response
			case "$response" in
				[yY]|[yY][eE][sS])
					case "$provider" in
						"docker")
							# Run ansible playbook inside container
							docker_cmd exec "$(printf '%q' "$container_name")" bash -c "
								ansible-playbook -i localhost, -c local \\
								/vm-tool/shared/ansible/playbook.yml
							"
							;;
						"vagrant")
							# Find the temp Vagrant directory
							local vagrant_dir
							vagrant_dir=$(find /tmp -maxdepth 1 -type d -name "vagrant-temp-${container_name}.*" 2>/dev/null | head -1)
							
							if [[ -n "$vagrant_dir" ]] && [[ -d "$vagrant_dir" ]]; then
								cd "$vagrant_dir"
								vagrant provision "$container_name"
							else
								echo "❌ Vagrant temp VM directory not found"
								exit 1
							fi
							;;
						*)
							echo "❌ Unsupported provider: $provider" >&2
							exit 1
							;;
					esac
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
	
	if [[ ${#MOUNT_ARGS[@]} -eq 0 ]]; then
		echo "❌ Usage: vm temp <mounts> [--auto-destroy]"
		echo "Example: vm temp ./src ./tests ./docs"
		exit 1
	fi
	
	# Check for backward compatibility with comma-separated mounts
	if [[ ${#MOUNT_ARGS[@]} -eq 1 ]] && [[ "${MOUNT_ARGS[0]}" == *,* ]]; then
		echo "⚠️  Warning: Comma-separated mounts are deprecated"
		echo "   Please use: vm temp ${MOUNT_ARGS[0]//,/ }"
		echo ""
		# Convert comma to array for processing
		old_ifs="$IFS"
		IFS=','
		IFS=',' read -ra MOUNT_ARGS <<< "${MOUNT_ARGS[0]}"
		IFS="$old_ifs"
	fi
	
	# Validate mount directories exist
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG handle_temp_command: validating ${#MOUNT_ARGS[@]} mounts" >&2
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
		
		if [[ "${VM_DEBUG:-}" = "true" ]]; then
			echo "DEBUG handle_temp_command: checking mount: '$source' (original: '$original_mount', perm: '$perm')" >&2
			echo "DEBUG handle_temp_command: directory exists: $([ -d "$source" ] && echo "yes" || echo "no")" >&2
			echo "DEBUG handle_temp_command: full path would be: $(realpath "$source" 2>/dev/null || echo "INVALID")" >&2
		fi
		
		if [[ ! -d "$source" ]]; then
			echo "❌ Error: Directory '$source' does not exist"
			echo "📂 Current directory: $(pwd)"
			echo "💡 Make sure the directory exists or use an absolute path"
			exit 1
		fi
		
		# Store processed mount with permission
		PROCESSED_MOUNTS+=("$source:$perm")
	done
	
	# Check for existing temp VM
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG handle_temp_command: about to get temp container name" >&2
		if type -t get_temp_container_name >/dev/null 2>&1; then
			echo "DEBUG handle_temp_command: function get_temp_container_name is defined" >&2
		else
			echo "DEBUG handle_temp_command: ERROR - function get_temp_container_name is NOT defined!" >&2
		fi
	fi
	# Get container name using shared function
	existing_container=""
	if [[ -f "$TEMP_STATE_FILE" ]]; then
		existing_container=$(get_container_name_from_state)
	fi
	TEMP_RET=$?
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG handle_temp_command: get_temp_container_name returned: $TEMP_RET" >&2
		echo "DEBUG handle_temp_command: existing_container='$existing_container'" >&2
		echo "DEBUG handle_temp_command: checking if container exists and is running" >&2
		echo "DEBUG handle_temp_command: about to check if block condition" >&2
	fi
	if [[ -n "$existing_container" ]] && is_temp_vm_running "$existing_container"; then
		if [[ "${VM_DEBUG:-}" = "true" ]]; then
			echo "DEBUG handle_temp_command: inside if block - existing container found" >&2
		fi
		# Active temp VM exists - check if mounts match
		existing_mounts=$(get_temp_mounts)
		
		# Normalize the requested mounts for comparison
		normalized_mounts=""
		for mount in "${PROCESSED_MOUNTS[@]}"; do
			source="${mount%:*}"
			perm="${mount##*:}"
			abs_source="$(realpath "$source" 2>/dev/null || echo "$source")"
			if [[ -n "$normalized_mounts" ]]; then
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
			if [[ "$AUTO_DESTROY" = "true" ]]; then
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
			if command -v yq &> /dev/null && [[ -f "$TEMP_STATE_FILE" ]]; then
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
		if [[ "${VM_DEBUG:-}" = "true" ]]; then
			echo "DEBUG handle_temp_command: no existing container or not running" >&2
		fi
	fi
	
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG handle_temp_command: proceeding to create new temp VM" >&2
	fi
	
	# No existing VM or user chose to recreate - proceed with creation
	# Generate unique VM name with timestamp
	local timestamp
	timestamp=$(date +%s)
	TEMP_CONTAINER="vmtemp-${timestamp}"
	
	# Route to provider-specific creation logic
	echo "🚀 Creating temporary VM with provider: $temp_provider"
	
	case "$temp_provider" in
		"docker")
			# Use existing Docker creation logic (unchanged)
			create_docker_temp_vm "$TEMP_CONTAINER" "${PROCESSED_MOUNTS[@]}"
			;;
		"vagrant")
			# Use new Vagrant creation logic
			create_vagrant_temp_vm "$TEMP_CONTAINER" "${PROCESSED_MOUNTS[@]}"
			;;
		*)
			echo "❌ Unsupported provider: $temp_provider" >&2
			exit 1
			;;
	esac
}

#=============================================================================
# PROVIDER-SPECIFIC TEMP VM CREATION FUNCTIONS
#=============================================================================

# Create Docker temp VM (extracted from original logic)
create_docker_temp_vm() {
	local container_name="$1"
	shift
	local mount_array=("$@")
	
	# Generate minimal temporary vm.yaml config with just overrides
	TEMP_CONFIG_FILE=$(mktemp /tmp/vm-temp.XXXXXX.yaml)
	
	# Setup cleanup function for interruptions
	cleanup_on_interrupt() {
		echo ""
		echo "🛑 Interrupted! Cleaning up..."
		# Remove temp container if it was created
		if [[ -n "${container_name:-}" ]]; then
			docker_cmd rm -f "$container_name" >/dev/null 2>&1 || true
		fi
		# Clean up volumes
		docker_cmd volume rm vmtemp_nvm vmtemp_cache vmtemp_config 2>/dev/null || true
		# Clean up network
		docker_cmd network rm vm-temp-project_vmtemp_network 2>/dev/null || true
		# Remove state file
		rm -f "$TEMP_STATE_FILE"
		# Remove temp files
		rm -f "$TEMP_CONFIG_FILE"
		# Safely remove temp project directory with validation
		if [[ -n "$TEMP_PROJECT_DIR" ]] && [[ "$TEMP_PROJECT_DIR" == "/tmp/vm-temp-project" ]]; then
			rm -rf "$TEMP_PROJECT_DIR"
		else
			echo "⚠️  Warning: Skipping cleanup of unexpected temp dir: $TEMP_PROJECT_DIR" >&2
		fi
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
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG handle_temp_command: extracting schema defaults from: $SCRIPT_DIR/vm.schema.yaml" >&2
	fi
	
	if ! SCHEMA_DEFAULTS=$("$SCRIPT_DIR/validate-config.sh" --extract-defaults "$SCRIPT_DIR/vm.schema.yaml" 2>&1); then
		echo "❌ Failed to extract schema defaults"
		echo "📋 Error output: $SCHEMA_DEFAULTS"
		rm -f "$TEMP_CONFIG_FILE"
		exit 1
	fi
	
	# Use yq to merge schema defaults with temp config
	if ! CONFIG=$(yq -s '.[0] * .[1]' <(echo "$SCHEMA_DEFAULTS") "$TEMP_CONFIG_FILE" 2>&1); then
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
	if [[ -n "$XDG_RUNTIME_DIR" ]] && mkdir -p "$XDG_RUNTIME_DIR/vm" 2>/dev/null; then
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
	echo "🚀 Creating Docker temporary VM with full provisioning..."
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG create_docker_temp_vm: calling docker_up with:" >&2
		echo "DEBUG create_docker_temp_vm:   CONFIG: [truncated for brevity]" >&2
		echo "DEBUG create_docker_temp_vm:   TEMP_PROJECT_DIR: $TEMP_PROJECT_DIR" >&2
		echo "DEBUG create_docker_temp_vm:   VM_TEMP_MOUNTS: $VM_TEMP_MOUNTS" >&2
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
		# Safely remove temp project directory with validation
		if [[ -n "$TEMP_PROJECT_DIR" ]] && [[ "$TEMP_PROJECT_DIR" == "/tmp/vm-temp-project" ]]; then
			rm -rf "$TEMP_PROJECT_DIR"
		else
			echo "⚠️  Warning: Skipping cleanup of unexpected temp dir: $TEMP_PROJECT_DIR" >&2
		fi
		return 1
	fi
	
	# Save temp VM state with Docker provider
	MOUNT_STRING=""
	for mount in "${mount_array[@]}"; do
		if [[ -n "$MOUNT_STRING" ]]; then
			MOUNT_STRING="$MOUNT_STRING,$mount"
		else
			MOUNT_STRING="$mount"
		fi
	done
	save_temp_state "$container_name" "$MOUNT_STRING" "$(pwd)" "docker"
	
	# Clean up temp config file only (keep project dir for mounts)
	rm -f "$TEMP_CONFIG_FILE"
	
	# Handle auto-destroy if flag was set
	if [[ "${AUTO_DESTROY:-}" = "true" ]]; then
		echo "🗑️ Auto-destroying temp VM..."
		docker_cmd rm -f "$container_name" >/dev/null 2>&1
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
	
	echo "✅ Docker temp VM created successfully!"
}

# Create Vagrant temp VM
create_vagrant_temp_vm() {
	local vm_name="$1"
	shift
	local mount_array=("$@")
	
	# Convert mount array to mount string for Vagrant config generator
	local mount_string=""
	for mount in "${mount_array[@]}"; do
		if [[ -n "$mount_string" ]]; then
			mount_string="$mount_string,$mount"
		else
			mount_string="$mount"
		fi
	done
	
	echo "🚀 Creating Vagrant temporary VM..."
	if [[ "${VM_DEBUG:-}" = "true" ]]; then
		echo "DEBUG create_vagrant_temp_vm: vm_name='$vm_name'" >&2
		echo "DEBUG create_vagrant_temp_vm: mount_string='$mount_string'" >&2
		echo "DEBUG create_vagrant_temp_vm: current_dir='$(pwd)'" >&2
	fi
	
	# Use Vagrant-specific creation function
	if ! vagrant_temp_create "$vm_name" "$mount_string" "$(pwd)"; then
		echo "❌ Failed to create Vagrant temporary VM" >&2
		return 1
	fi
	
	# Save temp VM state with Vagrant provider
	save_temp_state "$vm_name" "$mount_string" "$(pwd)" "vagrant"
	
	# Handle auto-destroy if flag was set
	if [[ "${AUTO_DESTROY:-}" = "true" ]]; then
		echo "🗑️ Auto-destroying temp VM..."
		vagrant_temp_destroy "$vm_name"
		# Remove state file
		rm -f "$TEMP_STATE_FILE"
	fi
	
	echo "✅ Vagrant temp VM created successfully!"
}
