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
vm --preset django create        # Force preset
vm --interactive create          # Choose interactively
vm --no-preset create            # Skip detection
vm --config prod.yaml create     # Custom config
```

### ssh
```bash
vm ssh                           # Enter VM
vm ssh -c "ls -la"              # Run command
vm ssh -c "npm test"            # Run tests
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
vm status                        # Check if running
vm list                          # Show all VMs
```

### logs
```bash
vm logs                          # View logs
```

### exec
```bash
vm exec "npm test"               # Run in VM
vm exec "python manage.py migrate"
vm exec "rake db:migrate"
```

### provision / kill
```bash
vm provision                     # Re-run setup scripts
vm kill                          # Force kill processes
```

---

## Temporary VMs

### temp create
```bash
vm temp ./src ./tests            # Mount folders
vm temp /absolute/path ./relative
vm temp ./configs:ro             # Read-only mount
vm tmp ./src                     # Alias for temp
```

### temp commands
```bash
vm temp ssh                      # Enter temp VM
vm temp ssh -c "npm test"       # Run command
vm temp status                   # Show status
vm temp destroy                  # Remove temp VM
```

### temp mounts
```bash
vm temp mount ./new-feature      # Add mount
vm temp unmount ./old-code      # Remove mount
vm temp mounts                   # List mounts
```

### temp lifecycle
```bash
vm temp start                    # Start stopped
vm temp stop                     # Stop (keep state)
vm temp restart                  # Restart
vm temp logs                     # View logs
vm temp list                     # Show all temp VMs
```

---

## Configuration

### init / validate
```bash
vm init                          # Create vm.yaml
vm init --preset django          # With preset
vm init --interactive            # Interactive setup

vm validate                      # Check vm.yaml
vm validate --config custom.yaml # Check specific file
vm validate --verbose            # Detailed output
```

### generate
```bash
vm generate                      # Interactive service selection
vm generate --services postgresql,redis
```

---

## Presets

### preset commands
```bash
vm preset list                   # Show all presets
vm preset list --verbose         # With descriptions
vm preset show django            # Show preset details
vm preset show react --yaml      # As YAML
```

**Available:** base, django, docker, kubernetes, nodejs, python, rails, react, tart-linux, tart-macos, tart-ubuntu

---

## Testing

### test
```bash
vm test                          # Run all tests
vm test --suite minimal          # Basic tests
vm test --suite services         # Service tests
vm test --suite integration      # Full tests
vm test --list                   # Show suites
vm test --verbose                # Detailed output
```

---

## Options & Environment

### Global Flags
```bash
vm --config <file> <command>    # Use specific config
vm --preset <name> create        # Force preset
vm --interactive create          # Interactive mode
vm --no-preset create            # Skip presets
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
vm temp ./src                    # Test environment
vm temp ssh -c "npm test"        # Run tests
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