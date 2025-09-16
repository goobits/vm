# 🚀 Goobits VM Infrastructure
Smart development environments that auto-configure based on your project type

## ✨ Key Features
- **🎯 Zero Configuration** - Detects React, Django, Rails, Vue, and 20+ frameworks automatically
- **🔐 AI Agent Safe** - Docker containers or full VM isolation for secure AI-assisted development
- **⚡ Instant Setup** - Docker environments in 10-30s, full VMs in 2-3min
- **🧪 Temporary VMs** - Quick experiments with specific folders mounted
- **🔄 File Sync** - Edit locally, run in VM with instant synchronization
- **📦 Smart Presets** - Auto-installs language runtimes, databases, and tools per project
- **🦀 Rust-Powered** - Core functionality migrated to Rust for improved performance

## 🚀 Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/goobits/vm.git
cd vm

# 2. Run the installer
# This script compiles and installs the `vm` binary
./install.sh

# 3. Configure your shell
# Restart your shell or source your profile to add `vm` to your PATH.
# e.g., source ~/.zshrc

# 4. Create an environment (auto-detects your project)
vm create

# 5. Enter your development environment
vm ssh
```

**Optional configuration:**
```yaml
# vm.yaml - only if you want to override defaults
os: ubuntu
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

## 🎮 Essential Commands

```bash
# Main workflow
vm create        # Create and configure VM
vm ssh           # Enter the VM
vm stop          # Stop VM (keeps data)
vm destroy       # Delete VM completely

# Quick experiments
vm temp ./src ./tests     # Instant VM with folder mounts
vm temp ssh               # Enter temp VM
vm temp destroy          # Clean up

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
vm temp ./src ./tests ./docs:ro
vm temp ssh              # Enter and start coding
vm temp destroy          # Clean up when done

# Dynamic mount management
vm temp mount ./new-feature    # Add directories while working
vm temp unmount ./old-code     # Remove when not needed
vm temp mounts                 # List current mounts
```

## ⚙️ Configuration

```yaml
# Minimal configuration
os: ubuntu

# Add ports
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

### Getting Started
- **[Quick Start Guide](docs/getting-started/quick-start.md)** - 5-minute setup tutorial
- **[Installation Guide](docs/getting-started/installation.md)** - Platform-specific setup
- **[Common Examples](docs/getting-started/examples.md)** - Real-world configurations

### User Guide
- **[CLI Reference](docs/user-guide/cli-reference.md)** - Complete command documentation
- **[Configuration Guide](docs/user-guide/configuration.md)** - Full configuration options
- **[Presets Guide](docs/user-guide/presets.md)** - Framework auto-detection
- **[Troubleshooting](docs/user-guide/troubleshooting.md)** - Common issues and solutions

### Development
- **[Contributing Guide](docs/development/contributing.md)** - How to contribute
- **[Architecture Overview](docs/development/architecture.md)** - System design
- **[Testing Guide](docs/development/testing.md)** - Test suite documentation

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
```

### Temporary VMs
```bash
vm temp <folders>            # Create ephemeral VM
vm temp ssh [-c cmd]         # SSH or run command
vm temp destroy              # Clean up
vm temp mount <path>         # Add mount to running VM
vm temp unmount <path>       # Remove mount
vm temp mounts               # List current mounts
```

### Configuration
```bash
vm init                      # Create vm.yaml
vm validate                  # Check config
vm preset list               # Show presets
vm preset show <name>        # Preset details
```

### Advanced
```bash
vm exec <command>            # Execute command in VM
vm logs                      # View logs
vm --config custom.yaml ssh # Use specific config
```

</details>