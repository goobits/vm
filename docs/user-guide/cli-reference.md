# 🛠️ VM CLI Reference

## 📚 Table of Contents
- [Core Commands](#core-commands)
- [Temporary VMs](#temporary-vms)
- [Configuration](#configuration)
- [Presets](#presets)
- [Testing](#testing)
- [Options & Environment](#options--environment)

---

## Core Commands

### create
```bash
vm create                        # Auto-detect project type
vm --config prod.yaml create     # Custom config
vm config preset django          # Apply preset to configuration
```

### ssh
```bash
vm ssh                           # Enter VM
vm ssh /workspace/src            # Enter VM in specific directory
vm exec ls -la                   # Run command (use exec instead)
vm exec npm test                 # Run tests (use exec instead)
```

### start / stop / restart
```bash
vm start                         # Start existing VM
vm stop                          # Stop (keep data)
vm restart                       # Restart VM
```

### destroy
```bash
vm destroy                       # Remove VM completely
```

### status / list
```bash
vm status                        # Show VM status and health
vm list                          # List all VMs with status and resource usage
```

### logs
```bash
vm logs                          # View VM logs
```

### exec
```bash
vm exec "npm test"               # Execute commands inside VM
vm exec "python manage.py migrate"
vm exec "rake db:migrate"
```

### provision
```bash
vm provision                     # Re-run VM provisioning
```

---

## Temporary VMs

### temp create
```bash
vm temp create ./src ./tests            # Create temporary VM with mounts
vm temp create /absolute/path ./relative
vm temp create ./configs:ro             # Read-only mount
```

### temp commands
```bash
vm temp ssh                      # Connect to temporary VM via SSH
vm temp status                   # Show temporary VM status
vm temp destroy                  # Destroy temporary VM
```

### temp mounts
```bash
vm temp mount ./new-feature      # Add mount to running temporary VM
vm temp unmount ./old-code       # Remove mount from temporary VM
vm temp mounts                   # List current mounts
```

### temp lifecycle
```bash
vm temp start                    # Start temporary VM
vm temp stop                     # Stop temporary VM
vm temp restart                  # Restart temporary VM
vm temp list                     # List all temporary VMs
```

---

## Configuration

### init / validate
```bash
vm init                          # Initialize a new VM configuration file
vm init --services postgresql,redis # With services
vm config preset django          # Apply preset after init

vm validate                      # Validate VM configuration
vm validate --config custom.yaml # Check specific file
```

### config
```bash
vm config set vm.memory 4096     # Set configuration value
vm config get                    # Get configuration values
vm config preset nodejs,docker   # Apply configuration presets
vm config preset list            # List all available presets
vm config preset --show nodejs   # Show specific preset details
```

---

## Presets

### preset commands
```bash
vm config preset nodejs          # Apply nodejs preset
vm config preset django,docker   # Apply multiple presets
vm config preset list            # List available presets
vm config preset --show django   # Show preset details
```

**Available presets:** base, django, docker, kubernetes, nodejs, python, rails, react, tart-linux, tart-macos, tart-ubuntu

**See:** [Presets Guide](./presets.md) for detailed preset information

---

## Testing

### Running Tests
```bash
# Run all Rust tests
cd rust && cargo test --workspace

# Run tests with output
cd rust && cargo test --workspace -- --nocapture

# Run specific package tests
cd rust && cargo test --package vm-config
```

---

## Options & Environment

### Global Flags
```bash
vm --config <file> <command>    # Use specific config
vm --dry-run create              # Show what would run
vm config preset <name>         # Apply preset to configuration
```

### Environment Variables
```bash
LOG_LEVEL=DEBUG vm create        # Debug output
VM_DEBUG=true vm create          # Bash debug mode
VM_PROVIDER=docker vm create     # Force provider
VM_CONFIG=custom.yaml vm ssh     # Default config
VM_MEMORY=8192 vm create         # Memory (MB)
VM_CPUS=4 vm create              # CPU count
```

### Config Priority
1. `--config` flag
2. `$VM_CONFIG` env
3. `./vm.yaml`
4. `./.vm/config.yaml`
5. `~/.vm/default.yaml`

### Exit Codes
- `0` - Success
- `1` - General error
- `2` - Config error
- `3` - Provider unavailable
- `4` - VM exists
- `5` - VM not found
- `10` - Port conflict

---

## Quick Examples

```bash
# Development workflow
vm create && vm ssh              # Start working
vm stop                          # End of day
vm start && vm ssh               # Resume
vm destroy                       # Cleanup

# Quick experiment
vm temp create ./src             # Test environment
vm temp ssh                      # Enter environment
vm temp destroy                  # Cleanup

# Multiple environments
vm --config dev.yaml create
vm --config staging.yaml create
vm --config prod.yaml create

# Debugging
LOG_LEVEL=DEBUG vm create 2>debug.log
vm exec "cat /etc/os-release"
docker inspect $(vm status --raw)
```

---

## Config Examples

```yaml
# Minimal (vm.yaml)
os: ubuntu

# With services
os: ubuntu
services:
  postgresql:
    enabled: true
  redis:
    enabled: true

# Full control
provider: docker
os: ubuntu
vm:
  memory: 4096
  cpus: 2
ports:
  web: 8000
  db: 5432
mounts:
  - ./src:/workspace/src
  - ./tests:/workspace/tests:ro
```