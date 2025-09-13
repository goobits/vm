# 🚀 Goobits VM Infrastructure

Beautiful development environments with one command. **Minimal configuration** - just run `vm create` and get a perfectly configured environment based on your project type.

> **🔐 Built for AI Agents**: Safe sandboxes for AI-assisted development with two isolation levels:
> - **Docker (default)**: Fast, lightweight containers (configurable 1-4GB RAM)
> - **Vagrant**: Full VM isolation with separate kernel for maximum security

## 🏃 Quick Start

### Option 1: Install from npm
```bash
# Requires: yq (install with: brew install yq)
npm install -g @goobits/vm

vm create  # Detects your project type and configures everything
vm ssh     # Enter your development environment
```

### Option 2: Install from source
```bash
# Clone the repository
git clone https://github.com/goobits/vm.git
cd vm

# Run installer (checks dependencies and guides you)
./install.sh

# Start using
vm create  # Detects your project type and configures everything
vm ssh     # Enter your development environment
```

**Minimal configuration needed:**
```yaml
# Create a vm.yaml with just:
os: ubuntu  # Everything else auto-configured!
```

**That's it!** The tool automatically detects React, Django, Rails, Vue, and 20+ other frameworks, then configures the perfect environment with all the tools you need.

📖 **Troubleshooting:** See the [Installation Guide](INSTALLATION.md) for detailed instructions.

## 🎯 How It Works

**Smart Preset System**: Analyzes your project files and automatically configures the perfect environment.

```bash
# Detects React project → installs Node.js, npm, dev server, testing tools
cd my-react-app && vm create

# Detects Django project → installs Python, PostgreSQL, Redis, Django tools  
cd my-api && vm create

# Multiple technologies → applies multiple presets intelligently
cd fullstack-app && vm create  # React frontend + Django backend
```

**Override when needed:**
```bash
vm --preset django create        # Force specific preset
vm --interactive create          # Choose presets interactively
vm --no-preset create            # Manual configuration only
```

📖 **Available presets**: React, Vue, Django, Rails, Node.js, Python, Rust, Docker, Kubernetes, and more. See [PRESETS.md](PRESETS.md).

## 🧪 Choose Your Environment

### Simple Way: Just Pick Your OS
```yaml
# Just specify the OS - provider auto-selected!
os: ubuntu   # → Docker/Vagrant, 4GB RAM, full dev stack
os: macos    # → Tart on Apple Silicon, 8GB RAM
os: debian   # → Docker/Vagrant, 2GB RAM, lightweight
os: alpine   # → Docker, 1GB RAM, minimal
os: linux    # → Docker/Vagrant, 4GB RAM, generic Linux
```

### Advanced: Explicit Provider Control
When you need specific provider features:

| Provider | Best For | Setup Time | Memory Allocation |
|----------|----------|------------|-------------------|
| **Docker** | Daily development | ⚡ 10-30s | 1-4GB (OS dependent) |
| **Vagrant** | Full isolation | 🐌 2-3min | 2-8GB (configurable) |
| **Tart** | Apple Silicon native | ⚡ 30-60s | 4-8GB (OS dependent) |

```bash
# Force specific provider (advanced)
echo "provider: vagrant" > vm.yaml && vm create
```

## 🎮 Essential Commands

**Main workflow:**
```bash
vm create        # Create and configure VM based on your project
vm ssh           # Enter the VM  
vm stop          # Stop VM (keeps data)
vm destroy       # Delete VM completely
```

**Quick experiments:**
```bash
vm temp ./src ./tests     # Instant VM with specific folders mounted
vm temp ssh               # Enter temp VM
vm temp destroy          # Clean up
```

**Project management:**
```bash
vm list          # Show all VMs
vm status        # Check if running
vm logs          # View container/VM logs
vm exec "cmd"    # Run command in VM
```

**Configuration:**
```bash
vm init          # Create vm.yaml config file
vm validate      # Check config
vm preset list   # Show available presets
```

## 🔄 Shell Integration

When you SSH into a VM and change directories, those changes are lost when you exit back to your host shell. This is a fundamental limitation - the VM can't change your parent shell's current directory.

**The Problem:**
```bash
vm ssh
# Inside VM: cd /workspace/src/components  
# Exit VM
pwd  # Still in original directory, not /workspace/src/components
```

**The Solution:** Use the `vm-cd` shell function to sync directories after exiting SSH.

### Setup

Add this function to your `~/.bashrc` or `~/.zshrc`:

```bash
vm-cd() {
    local sync_dir=$(vm get-sync-directory 2>/dev/null)
    [[ -n "$sync_dir" ]] && cd "$sync_dir"
}
```

### Usage

```bash
vm ssh
# Inside VM: cd src/components && exit
vm-cd  # Now you're in ./src/components on your host!
```

The VM automatically tracks your last directory when you exit SSH. The `vm get-sync-directory` command retrieves the corresponding host path, and `vm-cd` changes to that directory.

**Note:** This feature only works with the Docker provider, as it requires container-level directory tracking.

## ⚙️ Optional Configuration

**Works without any config**, but you can customize with `vm.yaml`:

### Simple Configuration
```yaml
# Minimal - just choose your OS!
os: ubuntu

# Add ports if needed
ports:
  frontend: 3000
  backend: 3001
```

### Advanced Configuration
```yaml
# Full control when you need it
provider: docker  # Explicit provider choice
project:
  name: my-project
  hostname: dev.my-project.local
services:
  postgresql:
    enabled: true
vm:
  memory: 4096    # 4GB RAM
  cpus: 2
```

📖 **Full guides**: [Configuration](CONFIGURATION.md) | [Presets](PRESETS.md) | [Installation](INSTALLATION.md)

## 📋 Complete Command Reference

<details>
<summary><strong>Click to expand all commands</strong></summary>

### Main VM Lifecycle
```bash
vm create                    # Create new VM with full provisioning
vm start                     # Start existing VM without provisioning  
vm stop                      # Stop VM but keep data
vm restart                   # Restart VM without reprovisioning
vm ssh                       # Connect to VM
vm destroy                   # Delete VM completely
vm status                    # Check if running
vm kill                      # Force kill stuck processes
vm provision                 # Re-run provisioning
```

### Configuration & Setup
```bash
vm init                      # Initialize new vm.yaml config file
vm generate                  # Generate vm.yaml by composing services
vm validate                  # Check config
vm list                      # List all VM instances
```

### Temporary VMs
```bash
vm temp <folders>            # Create ephemeral VM with directory mounts
vm temp ssh [-c cmd]         # SSH into temp VM or run command
vm temp status               # Show temp VM status and configuration  
vm temp destroy              # Destroy temp VM and clean up state
vm temp mount <path>         # Add mount to running temp VM
vm temp unmount <path>       # Remove mount from running temp VM
vm temp mounts               # List current mounts
vm temp list                 # List active temp VM instances
vm temp start                # Start stopped temp VM
vm temp stop                 # Stop temp VM (preserves state)
vm temp restart              # Restart temp VM
vm temp logs                 # View container logs
vm temp provision            # Re-run provisioning
vm tmp <folders>             # Alias for vm temp
```

### Presets & Project Detection
```bash
vm preset list               # List all available presets
vm preset show <name>        # Show detailed preset configuration
vm --preset <name> create    # Force specific preset
vm --interactive create      # Interactive preset selection
vm --no-preset create        # Disable preset detection
```

### Advanced Usage
```bash
vm logs                      # View service logs
vm exec <command>            # Execute command in VM
vm test                      # Run all tests
vm test --suite minimal     # Run specific test suite
vm test --suite services    # Test service configurations
vm test --list              # Show available test suites
vm --config prod.yaml create # Create with specific config
vm --config dev.yaml ssh     # Any command works with --config
```

</details>

## 🚀 Temporary VMs

**Perfect for quick experiments** - no configuration needed, instant setup:

```bash
# Mount specific directories and get working environment
vm temp ./src ./tests ./docs:ro
vm temp ssh              # Enter and start coding
vm temp destroy          # Clean up when done

# Dynamic mount management
vm temp mount ./new-feature    # Add directories while working  
vm temp unmount ./old-code     # Remove when not needed
vm temp mounts                 # List current mounts
```

**Use cases**: Code reviews, testing libraries, debugging, trying new tools in isolation.



## 📦 What's Included

- **Ubuntu 24.04 LTS** with Zsh and syntax highlighting
- **Smart preset system** with 20+ framework presets  
- **Language runtimes**: Node.js, Python, Rust (auto-installed based on your project)
- **Services**: PostgreSQL, Redis, MongoDB, Docker (configurable)
- **8 beautiful terminal themes** with git-aware prompts
- **Auto-sync**: Edit locally, run in VM with instant file sync
- **Package managers**: npm, pnpm, pip, cargo (as needed)

## 🚨 Common Issues

- **VM won't start?** → `vm destroy && vm create`
- **Port conflicts?** → Check output for remapped ports or adjust in vm.yaml  
- **Can't connect to database?** → All services use `localhost` (not container names)
- **Slow performance?** → Increase memory/CPUs in vm.yaml or switch to Docker
- **Need more help?** → Check `vm logs` or see [troubleshooting guide](INSTALLATION.md#troubleshooting)

## 📊 Logging & Debugging

The VM tool includes structured logging for production deployments and debugging:

```bash
# Set log level for more detailed output
LOG_LEVEL=DEBUG vm create
LOG_LEVEL=ERROR vm temp destroy  # Only show errors

# Container-friendly logging (INFO/DEBUG → stdout, WARN/ERROR → stderr)
vm create 2>errors.log 1>info.log
```

Available log levels: `DEBUG`, `INFO` (default), `WARN`, `ERROR`

---

**Files are synced instantly** between your local machine and VM. Edit locally, run in VM - it just works! 🎪