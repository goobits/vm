# 🚀 Goobits VM
Development environments that auto-configure based on your project type

## ✨ Key Features
- **🎯 Zero Configuration** - Detects React, Django, Rails, Vue, Angular, Next.js, Flask and more frameworks automatically
- **🔐 AI Agent Safe** - Docker containers with secure defaults prevent host escape
- **⚡ Instant Setup** - Docker environments in 10-30s, full VMs in 2-3min
- **🧪 Temporary VMs** - Quick experiments with specific folders mounted
- **🔄 File Sync** - Edit locally, run in VM with instant synchronization
- **📦 Auto Presets** - Installs language runtimes, databases, and tools per project

## 🚀 Quick Start

```bash
# Installation
git clone https://github.com/goobits/vm.git
cd vm && ./install.sh

# Restart shell or source profile
source ~/.zshrc

# Create environment (auto-detects your project)
vm create

# Enter development environment
vm ssh
```

```yaml
# Optional: vm.yaml for custom configuration
os: ubuntu
provider: docker  # or vagrant for full isolation
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
vm list          # Show all VMs
vm status        # Check if running
vm logs          # View logs
vm exec "cmd"    # Run command in VM

# Configuration
vm init          # Create vm.yaml
vm validate      # Check config
vm preset list   # Available presets
```

## 🧪 Temporary VMs

```bash
# Quick experiments with specific folder mounts
vm temp create ./src ./tests ./docs:ro
vm temp ssh              # Enter and start coding
vm temp destroy          # Clean up when done

# Dynamic mount management
vm temp mount ./new-feature     # Add directories while working
vm temp unmount ./old-code      # Remove specific mount
vm temp unmount --all           # Remove all mounts
vm temp mounts                  # List current mounts
```

## ⚙️ Configuration

```yaml
# Minimal configuration
os: ubuntu

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
# View configurations
vm preset list               # Available presets
vm preset show django        # Preset details

# Override defaults
vm --preset django create        # Force specific preset
vm --no-preset create            # Manual configuration only
```

## 📖 Documentation
- **[Quick Start Guide](docs/getting-started/quick-start.md)** - 5-minute setup tutorial
- **[Installation Guide](docs/getting-started/installation.md)** - Platform-specific setup
- **[CLI Reference](docs/user-guide/cli-reference.md)** - Complete command documentation
- **[Configuration Guide](docs/user-guide/configuration.md)** - Full configuration options
- **[Presets Guide](docs/user-guide/presets.md)** - Framework auto-detection
- **[Troubleshooting](docs/user-guide/troubleshooting.md)** - Common issues and solutions
- **[Contributing Guide](docs/development/contributing.md)** - How to contribute
- **[Architecture Overview](docs/development/architecture.md)** - System design

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

## 🔄 Shell Integration (Advanced)

```bash
# Add to ~/.bashrc or ~/.zshrc for directory sync
vm-cd() {
    local sync_dir=$(vm get-sync-directory 2>/dev/null)
    [[ -n "$sync_dir" ]] && cd "$sync_dir"
}

# Usage: SSH, change directory, exit, then sync
vm ssh
# Inside VM: cd src/components && exit
vm-cd  # Now in ./src/components on host
```

<details>
<summary><strong>Complete Command Reference</strong></summary>

### VM Lifecycle
```bash
vm create                    # Create new VM with provisioning
vm start                     # Start existing VM
vm stop                      # Stop VM (keeps data)
vm restart                   # Restart without reprovisioning
vm ssh                       # Connect to VM
vm destroy                   # Delete VM completely
vm status                    # Check if running
vm provision                 # Re-run provisioning
vm kill [container]          # Force kill VM processes
```

### Temporary VMs
```bash
vm temp create <folders>     # Create ephemeral VM
vm temp ssh                  # SSH into temp VM
vm temp destroy              # Clean up
vm temp mount <path>         # Add mount to running VM
vm temp unmount <path>       # Remove specific mount
vm temp unmount --all        # Remove all mounts
vm temp mounts               # List current mounts
vm temp list                 # List all temp VMs
vm temp status               # Check temp VM status
vm temp stop                 # Stop temp VM
vm temp start                # Start temp VM
vm temp restart              # Restart temp VM
```

### Configuration
```bash
vm init                      # Create vm.yaml
vm validate                  # Check config
vm preset list               # Show presets
vm preset show <name>        # Preset details
vm config get [field]        # Get config value(s)
vm config set <field> <value> # Set config value
vm config unset <field>      # Remove config field
vm config clear              # Clear all config
vm config preset <names>     # Apply preset(s)
```

### Advanced
```bash
vm exec <command>            # Execute command in VM
vm logs                      # View logs
vm --config custom.yaml ssh # Use specific config
```

</details>