# VM Configuration Migration & Versioning Proposal

## Overview

This proposal outlines the implementation of configuration migration tools and versioning for the VM infrastructure project. It introduces:
1. A `vm migrate` command to help users transition from JSON to YAML configurations
2. Configuration versioning for future compatibility
3. Enhanced `vm temp` commands with `vm tmp` alias
4. Quick SSH access to temporary VMs

## Motivation

With the completion of the YAML migration project, users need:
- A simple way to migrate existing JSON configurations to YAML
- Version tracking for configuration formats
- Convenient shortcuts for common operations
- Better temporary VM management

## Proposed Features

### 1. Configuration Versioning

Add a `version` field to track configuration format versions:

```yaml
# vm.yaml
version: "1.0"  # Configuration format version
project:
  name: my-app
  hostname: dev.my-app.local
  # ... rest of configuration
```

**Version History:**
- `1.0` - Initial YAML format with full feature set
- Future versions will maintain backward compatibility

**Benefits:**
- Safe migrations between versions
- Feature detection capabilities
- Clear compatibility requirements
- Automated upgrade paths

### 2. `vm migrate` Command

A new command to migrate JSON configurations to YAML format:

```bash
vm migrate [options]
```

**Options:**
- `--input <path>` - Source JSON file (default: auto-detect vm.json)
- `--output <path>` - Target YAML file (default: vm.yaml)
- `--backup` - Create backup of original (default: true)
- `--dry-run` - Preview changes without applying
- `--force` - Skip confirmation prompts
- `--check` - Check if migration is needed

**Migration Workflow:**
1. Auto-discover JSON configuration
2. Validate source configuration
3. Create backup (unless disabled)
4. Convert to YAML with version 1.0
5. Validate resulting YAML
6. Show summary and confirm
7. Optionally remove source JSON

**Example Usage:**
```bash
# Basic migration
vm migrate

# Specific files
vm migrate --input legacy.json --output vm.yaml

# Preview changes
vm migrate --dry-run

# Check if needed
vm migrate --check
```

**Example Output:**
```
🔄 VM Configuration Migration (JSON → YAML)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📁 Source: /home/user/project/vm.json (no version)
📄 Target: /home/user/project/vm.yaml (version: 1.0)
💾 Backup: /home/user/project/vm.json.bak

Analyzing configuration...
✅ JSON validation passed
✅ No compatibility issues found

📊 Configuration Summary:
- Project: my-app
- Provider: docker
- Services: postgresql, redis
- Ports: 3000, 3001, 5432, 6379
- Version: none → 1.0

⚠️  This will create vm.yaml. Continue? [Y/n]

✅ Migration successful!
📝 Created: vm.yaml (version 1.0)
💾 Backup: vm.json.bak

🚮 Remove original vm.json? [y/N]
```

### 3. Enhanced Temp VM Commands

Add `vm tmp` as an alias and implement SSH functionality:

**New Commands:**
```bash
# Create temp VM (both work identically)
vm temp ./src,./docs
vm tmp ./src,./docs

# SSH to current temp VM
vm temp ssh
vm tmp ssh

# Check temp VM status
vm temp status
vm tmp status

# List temp VMs
vm temp list
vm tmp list

# Destroy temp VM
vm temp destroy
vm tmp destroy
```

**Temp VM State Tracking:**

Store temp VM information in `~/.vm/temp-vm.state`:
```yaml
container_name: vm-temp
created_at: 2024-01-20T10:30:00Z
mounts:
  - path: ./src
    permission: rw
  - path: ./docs
    permission: ro
project_dir: /home/user/project
```

**Smart Collision Detection:**

When creating a temp VM, the command checks for existing temp VMs:

1. **Same mounts** → Connect to existing VM
2. **Different mounts** → Show conflict and options
3. **No temp VM** → Create new one

**Example Workflows:**

```bash
# First time - creates new temp VM
$ vm tmp ./client,./server
🚀 Creating temporary VM...
✅ Temp VM created: vm-temp
📁 Mounted: ./client (rw), ./server (rw)

# Same command again - connects to existing
$ vm tmp ./client,./server
🔍 Found existing temp VM with same mounts
🎯 Connecting to vm-temp...
developer@vm-temp:~$ 

# Different mounts - shows conflict
$ vm tmp ./frontend,./backend
⚠️  Temp VM already exists with different mounts!

Current temp VM:
📦 Container: vm-temp (running)
📁 Mounts: ./client (rw), ./server (rw)
⏱️  Uptime: 45 minutes

Options:
1. Connect to existing temp VM anyway
2. Destroy and create new temp VM
3. Cancel

Choose [1-3]: 2
🗑️  Destroying existing temp VM...
🚀 Creating new temp VM...
✅ Temp VM created: vm-temp
📁 Mounted: ./frontend (rw), ./backend (rw)

# Quick SSH to current temp VM
$ vm tmp ssh
🎯 Connecting to temp VM...
developer@vm-temp:~$ 

# Check status
$ vm tmp status
✅ Temp VM Status
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📦 Container: vm-temp (running)
📁 Mounts: ./frontend (rw), ./backend (rw)
⏱️  Uptime: 2 hours 15 minutes
📍 Project: /home/user/project
```

## Implementation Plan

### Phase 1: Configuration Versioning
1. Update schema to include version field
2. Add version to default configurations
3. Update validation to handle version
4. Add version checking to scripts

### Phase 2: Migration Command
1. Add `migrate` command to vm.sh
2. Implement JSON to YAML conversion
3. Add version injection during migration
4. Create backup and cleanup logic
5. Add dry-run and check modes

### Phase 3: Temp VM Enhancements
1. Add `tmp` alias to command parser
2. Implement state file management
3. Add `ssh` subcommand
4. Add `status` and `list` subcommands
5. Update help documentation

### Phase 4: Testing & Documentation
1. Add migration tests
2. Test version compatibility
3. Test temp VM state management
4. Update README with new commands
5. Add migration guide

## Technical Details

### Version Checking Implementation
```bash
# Get version with fallback
CONFIG_VERSION=$(yq -r '.version // "1.0"' vm.yaml)

# Version comparison
version_compare() {
    # Compare semantic versions
    local v1=$1 v2=$2
    # Implementation details...
}

# Check minimum version
require_version() {
    local min_version=$1
    local current=$(yq -r '.version // "1.0"' vm.yaml)
    if ! version_compare "$current" ">=" "$min_version"; then
        echo "❌ This feature requires version $min_version or higher"
        exit 1
    fi
}
```

### Migration Safety Checks
- Validate JSON before migration
- Check for incompatible settings
- Preserve all configuration data
- Maintain comments where possible
- Validate YAML after migration

### State File Management
```bash
# Save temp VM state
save_temp_state() {
    local mounts_array="$1"
    cat > ~/.vm/temp-vm.state << EOF
container_name: vm-temp
created_at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
mounts: $mounts_array
project_dir: $(pwd)
pid: $$
EOF
}

# Read temp VM state
get_temp_vm_info() {
    if [[ -f ~/.vm/temp-vm.state ]]; then
        yq -r . ~/.vm/temp-vm.state
    fi
}

# Check if mounts match
mounts_match() {
    local requested_mounts="$1"
    local current_mounts=$(yq -r '.mounts' ~/.vm/temp-vm.state 2>/dev/null)
    
    # Normalize and compare mount lists
    local req_normalized=$(echo "$requested_mounts" | tr ',' '\n' | sort | tr '\n' ',')
    local cur_normalized=$(echo "$current_mounts" | tr ',' '\n' | sort | tr '\n' ',')
    
    [[ "$req_normalized" == "$cur_normalized" ]]
}

# Handle temp VM collision
handle_temp_collision() {
    local requested_mounts="$1"
    
    # Check if temp VM exists
    if ! docker ps -q -f name=vm-temp &>/dev/null; then
        return 1  # No collision
    fi
    
    # Check if mounts match
    if mounts_match "$requested_mounts"; then
        echo "🔍 Found existing temp VM with same mounts"
        echo "🎯 Connecting to vm-temp..."
        return 0  # Same mounts, just connect
    fi
    
    # Different mounts - show options
    echo "⚠️  Temp VM already exists with different mounts!"
    echo ""
    show_temp_vm_info
    echo ""
    echo "Options:"
    echo "1. Connect to existing temp VM anyway"
    echo "2. Destroy and create new temp VM"  
    echo "3. Cancel"
    echo ""
    
    read -p "Choose [1-3]: " choice
    case $choice in
        1) return 0 ;;  # Connect anyway
        2) vm_temp_destroy && return 1 ;;  # Destroy and recreate
        *) exit 0 ;;  # Cancel
    esac
}
```

## Migration Guide for Users

### For Users with Existing JSON Configs
```bash
# 1. Check current configuration
vm validate

# 2. Migrate to YAML
vm migrate

# 3. Verify migration
vm validate

# 4. Test with new config
vm status

# 5. Remove old JSON (optional)
rm vm.json.bak
```

### For New Users
- Start directly with YAML format
- Use `vm init` to create vm.yaml
- Version 1.0 is automatically applied

## Future Considerations

### Version 1.1+ Features
- Environment-specific overrides
- Service dependency management
- Advanced networking options
- Plugin system support

### Backward Compatibility
- Always support reading older versions
- Provide automatic upgrade paths
- Clear deprecation warnings
- Migration tools for major versions

## Summary

This proposal provides:
1. **Smooth Migration Path**: Easy transition from JSON to YAML
2. **Future-Proofing**: Version tracking for compatibility
3. **Improved UX**: Shorter commands and convenient shortcuts
4. **Better Temp VMs**: State tracking and quick SSH access

The implementation maintains backward compatibility while setting up the project for future enhancements.