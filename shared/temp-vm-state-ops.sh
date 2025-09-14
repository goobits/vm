#!/bin/bash
# Simple state file operations for temp VM without yq dependency
# Uses vm-config for all YAML operations

set -e

# Get the vm-config binary path
get_vm_config() {
  if command -v vm-config &> /dev/null; then
    echo "vm-config"
  elif [[ -x "${VM_TOOL_DIR:-/workspace}/rust/target/release/vm-config" ]]; then
    echo "${VM_TOOL_DIR:-/workspace}/rust/target/release/vm-config"
  else
    echo "❌ vm-config binary not found" >&2
    exit 1
  fi
}

VM_CONFIG=$(get_vm_config)

# Check if mount exists in state file
mount_exists() {
  local state_file="$1"
  local mount_path="$2"

  if [[ ! -f "$state_file" ]]; then
    return 1
  fi

  # Use vm-config to check if mount exists
  $VM_CONFIG select-where "$state_file" "mounts" "source" "$mount_path" -f yaml 2>/dev/null | grep -q "source:"
}

# Get mount count from state file
get_mount_count() {
  local state_file="$1"

  if [[ ! -f "$state_file" ]]; then
    echo "0"
    return
  fi

  $VM_CONFIG array-length "$state_file" "mounts" 2>/dev/null || echo "0"
}

# Remove mount from state file
remove_mount() {
  local state_file="$1"
  local mount_path="$2"

  if [[ ! -f "$state_file" ]]; then
    return 1
  fi

  local temp_file
  temp_file=$(mktemp)

  # Use vm-config delete command
  $VM_CONFIG delete "$state_file" "mounts" "source" "$mount_path" -f yaml > "$temp_file"
  mv "$temp_file" "$state_file"
}

# List mounts from state file
list_mounts() {
  local state_file="$1"
  local format="${2:-simple}"

  if [[ ! -f "$state_file" ]]; then
    return
  fi

  case "$format" in
    simple)
      # Just list source paths
      $VM_CONFIG query "$state_file" "mounts" -f yaml 2>/dev/null | grep "source:" | sed 's/.*source: //'
      ;;
    detailed)
      # Show source -> target (permissions)
      $VM_CONFIG query "$state_file" "mounts" -f yaml 2>/dev/null | awk '
        /^- source:/ { source = $3 }
        /  target:/ { target = $2 }
        /  permissions:/ {
          perms = $2
          printf "  📂 %s → %s (%s)\n", source, target, perms
        }
      '
      ;;
    bullets)
      # Show as bullet list
      $VM_CONFIG query "$state_file" "mounts" -f yaml 2>/dev/null | grep "source:" | sed 's/.*source: /  • /'
      ;;
  esac
}

# Check if state file has new format
has_new_format() {
  local state_file="$1"

  if [[ ! -f "$state_file" ]]; then
    return 1
  fi

  $VM_CONFIG has-field "$state_file" "mounts.0.source" 2>/dev/null | grep -q "true"
}

# Add mount to state file
add_mount() {
  local state_file="$1"
  local source="$2"
  local target="$3"
  local permissions="${4:-rw}"

  local temp_file
  temp_file=$(mktemp)

  # Create mount object
  cat > "$temp_file" <<EOF
source: "$source"
target: "$target"
permissions: "$permissions"
EOF

  # Add to array using vm-config
  $VM_CONFIG add-to-array "$state_file" "mounts" "$temp_file" -f yaml > "${state_file}.tmp"
  mv "${state_file}.tmp" "$state_file"
  rm -f "$temp_file"
}

# Export functions for use by other scripts
export -f get_vm_config
export -f mount_exists
export -f get_mount_count
export -f remove_mount
export -f list_mounts
export -f has_new_format
export -f add_mount