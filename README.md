# 🚀 Goobits VM
Development environments that auto-configure based on your project type

## 📚 Documentation

### Getting Started
- **[Quick Start Guide](docs/getting-started/quick-start.md)** - 5-minute setup tutorial
- **[Installation Guide](docs/getting-started/installation.md)** - Platform-specific setup
- **[Examples](docs/getting-started/examples.md)** - Common use cases and patterns

### User Guide
- **[CLI Reference](docs/user-guide/cli-reference.md)** - Complete command documentation
- **[Configuration Guide](docs/user-guide/configuration.md)** - Full configuration options
- **[Presets Guide](docs/user-guide/presets.md)** - Framework auto-detection
- **[Troubleshooting](docs/user-guide/troubleshooting.md)** - Common issues and solutions

### Development
- **[Contributing Guide](docs/development/contributing.md)** - How to contribute
- **[Testing Guide](docs/development/testing.md)** - Running and writing tests

## ✨ Key Features
- **Minimal Configuration** - Detects React, Django, Rails, Vue, Angular, Next.js, Flask and more frameworks automatically
- **Container Isolation** - Docker containers with secure defaults
- **Quick Setup** - Docker environments typically ready in under a minute
- **Temporary VMs** - Ephemeral environments with specific folder mounts
- **File Sync** - Edit locally, run in VM with file synchronization
- **Preset System** - Installs language runtimes, databases, and tools per project

## 🚀 Quick Start

```bash
# Installation (recommended)
cargo install vm

# Alternative: from source
git clone https://github.com/goobits/vm.git
cd vm && ./install.sh

# Create environment (auto-detects your project)
vm create

# Enter development environment
vm ssh
```

```yaml
# Optional: vm.yaml for custom configuration
os: ubuntu
provider: docker  # or vagrant for full isolation
project:
  name: my-project
```

## 🛠️ Environment Types

### Docker (Default)
```bash
# Lightweight containers (1-4GB RAM)
vm create  # Auto-selects Docker for most projects
```

### Vagrant (Full Isolation)
```yaml
# vm.yaml for maximum security
provider: vagrant
project:
  name: my-project
```

### Auto-Detection
```bash
# Detects project type and configures automatically
cd my-react-app && vm create     # → Node.js, npm, dev tools
cd my-django-api && vm create    # → Python, PostgreSQL, Redis
cd fullstack-app && vm create    # → Multiple presets combined
```

## 🎮 Commands

```bash
# Main workflow
vm create        # Create and configure VM
vm ssh           # Enter the VM
vm stop          # Stop VM (keeps data)
vm destroy       # Delete VM completely

# Quick experiments
vm temp create ./src ./tests     # Instant VM with folder mounts
vm temp ssh                      # Enter temp VM
vm temp destroy                  # Clean up

# Management
vm list          # List all VMs with status and resource usage
vm status        # Show VM status and health
vm logs          # View VM logs
vm exec "cmd"    # Execute commands inside VM

# Configuration
vm init          # Initialize a new VM configuration file
vm validate      # Validate VM configuration
```

## 🧪 Temporary VMs

```bash
# Quick experiments with specific folder mounts
vm temp create ./src ./tests ./docs:ro
vm temp ssh              # Enter and start coding
vm temp destroy          # Clean up when done

# Dynamic mount management
vm temp mount ./new-feature     # Add directories while working
vm temp unmount --path ./old-code # Remove specific mount
vm temp unmount --all           # Remove all mounts
vm temp mounts                  # List current mounts
```

## ⚙️ Configuration

```yaml
# Minimal configuration
os: ubuntu
project:
  name: my-project

# Add ports (mapped as key-value pairs)
ports:
  frontend: 3000
  backend: 3001

# Advanced settings
provider: docker
vm:
  memory: 4096
  cpus: 2
services:
  postgresql:
    enabled: true
    version: "15"
```

```bash
# Apply specific presets
vm config preset django          # Apply Django preset to config
vm config preset list            # List available presets
vm config preset --show nodejs   # Show specific preset details
```

## 🧪 Development

```bash
# Debugging
LOG_LEVEL=DEBUG vm create    # Detailed output
vm logs                      # View service logs
```

## 💡 Support
- **Issues**: Report at [GitHub Issues](https://github.com/goobits/vm/issues)
- **Troubleshooting**: See [Troubleshooting Guide](docs/user-guide/troubleshooting.md)
- **Quick fixes**: `vm destroy && vm create` for most problems

---

## 📖 Complete Command Reference

### VM Lifecycle
```bash
vm create                    # Create and provision a new VM
vm start                     # Start a VM
vm stop [container]          # Stop a VM or force-kill specific container
vm restart                   # Restart a VM
vm provision                 # Re-run VM provisioning
vm destroy                   # Destroy a VM and clean up resources
vm status                    # Show VM status and health
vm ssh                       # Connect to VM via SSH
```

### Temporary VMs
```bash
vm temp create <folders>     # Create temporary VM with mounts
vm temp ssh                  # Connect to temporary VM via SSH
vm temp status               # Show temporary VM status
vm temp destroy              # Destroy temporary VM
vm temp mount <path>         # Add mount to running temporary VM
vm temp unmount --path <path> # Remove specific mount from temporary VM
vm temp unmount --all        # Remove all mounts
vm temp mounts               # List current mounts
vm temp list                 # List all temporary VMs
vm temp stop                 # Stop temporary VM
vm temp start                # Start temporary VM
vm temp restart              # Restart temporary VM
```

### Configuration
```bash
vm init                      # Initialize a new VM configuration file
vm validate                  # Validate VM configuration
vm config get [field]        # Get configuration values
vm config set <field> <value> # Set configuration value
vm config unset <field>      # Remove configuration field
vm config preset <names>     # Apply configuration presets
vm config preset list        # List available presets
vm config preset --show <name> # Show preset details
vm config ports --fix        # Manage port configuration and resolve conflicts
```

### Advanced
```bash
vm list                      # List all VMs with status and resource usage
vm exec <command>            # Execute commands inside VM
vm logs                      # View VM logs
vm --config custom.yaml ssh # Use specific config
```