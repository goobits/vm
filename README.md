# 🚀 Goobits VM Infrastructure

Beautiful development environments with one command. Choose between Docker (lightweight containers, default) or Vagrant (full VM isolation) based on your needs.

> **🔐 Built for AI Agents**: This infrastructure provides safe sandboxes for AI-assisted development when you need system isolation. Choose your isolation level:
> - **Docker (default)**: Lightweight containers with shared kernel - fast and resource-efficient for most workloads
> - **Vagrant**: Full VM isolation with separate kernel - ideal for risky operations or when system security is a concern

## 🏃 Quick Start

```bash
# Install globally via npm
npm install -g @goobits/vm

# Start immediately with smart preset detection
vm create  # Automatically detects and configures for your project type!
vm ssh     # Enter your perfectly configured development environment
```

📖 **Need help installing?** See the complete [Installation Guide](INSTALLATION.md) for all installation options and troubleshooting.

## 🎯 Smart Preset System

The VM Tool includes an intelligent preset system that automatically configures virtual machines based on your project type. No more manual configuration - just point the tool at your project and get a perfectly configured development environment.

### Key Features

- **🔍 Automatic Detection**: Analyzes your project files to detect frameworks (React, Django, Rails, etc.)
- **⚡ One-Command Setup**: `vm create` automatically applies the right preset for your project
- **🎛️ Interactive Mode**: `vm --interactive create` lets you customize preset selection
- **📦 Multiple Presets**: Handles complex projects with multiple technologies (e.g., React + Docker)
- **🔧 Fully Customizable**: Override any preset setting in your `vm.yaml`

### Quick Start

```bash
# Automatic preset detection
cd my-react-project && vm create

# Interactive preset selection  
vm --interactive create

# Force specific preset
vm --preset django create

# Explore available presets
vm preset list
vm preset show react
```

For detailed preset documentation, see [PRESETS.md](PRESETS.md).

## ⚙️ Configuration

Create a `vm.yaml` file to customize your environment:

```yaml
project:
  name: my-project
ports:
  frontend: 3000
  backend: 3001
services:
  postgresql:
    enabled: true
```

📖 **Full configuration guide**: See [CONFIGURATION.md](CONFIGURATION.md) for complete reference, examples, and migration from JSON.

## 🎮 Commands

```bash
vm init                      # Initialize a new vm.yaml configuration file
vm generate                  # Generate vm.yaml by composing services and configurations
vm migrate                   # Convert vm.json to vm.yaml with version tracking
vm list                      # List all VM instances
vm temp <folders>            # Create ephemeral VM with specific directory mounts
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
vm create                    # Create new VM/container with full provisioning
vm start                     # Start existing VM/container without provisioning
vm stop                      # Stop VM/container but keep data
vm restart                   # Restart VM/container without reprovisioning
vm ssh                       # Connect to VM/container
vm destroy                   # Delete VM/container completely
vm status                    # Check if running
vm validate                  # Check config
vm kill                      # Force kill stuck processes
vm provision                 # Re-run provisioning

# Provider-specific commands
vm logs                      # View service logs (Docker: container logs, Vagrant: journalctl)
vm exec <command>            # Execute command in VM/container

# Testing
vm test                      # Run all tests
vm test --suite minimal     # Run specific test suite
vm test --suite services    # Test service configurations
vm test --list              # Show available test suites

# Preset commands
vm preset list               # List all available presets
vm preset show <name>        # Show detailed preset configuration
vm --preset <name> create    # Force specific preset
vm --interactive create      # Interactive preset selection
vm --no-preset create        # Disable preset detection

# Use custom config file
vm --config prod.yaml create # Create with specific config
vm --config dev.yaml ssh     # Any command works with --config
```

## 📚 Documentation

- 📖 [Installation Guide](INSTALLATION.md) - Complete installation instructions for all platforms
- ⚙️ [Configuration Reference](CONFIGURATION.md) - Full configuration options and examples
- 🎯 [Preset System Guide](PRESETS.md) - Smart preset system and customization options
- 🔄 [Changelog](CHANGELOG.md) - Recent updates and version history

## 📦 What's Included

- **Ubuntu 24.04 LTS** with Zsh + syntax highlighting
- **Node.js v22** via NVM (configurable)
- **pnpm** via Corepack
- **Beautiful terminals** with 8 themes
- **Smart preset system** with automatic project detection
- **Framework-specific environments**: React, Django, Rails, Vue, Next.js, and more
- **Interactive preset customization** for complex projects
- **Optional services**: PostgreSQL, Redis, MongoDB, Docker, Headless Browser
- **Auto-sync**: Edit locally, run in VM
- **Claude-ready**: Safe sandbox for AI experiments
- **Provider choice**: Docker (default, lightweight) or Vagrant (full isolation)  
- **Unified architecture**: Both providers use identical Ansible provisioning
- **Modular bash architecture**: Clean, maintainable scripts with extracted modules
- **Automatic language installation**: Rust (via cargo_packages) and Python (via pip_packages)
- **Configuration migration**: Easy upgrade from JSON to YAML with versioning

## 🎨 Terminal Themes

All themes include syntax highlighting and git-aware prompts!

- `dracula` ⭐ - Purple magic (default)
- `gruvbox_dark` - Retro warmth
- `solarized_dark` - Science-backed colors
- `nord` - Arctic vibes
- `monokai` - Classic vibrance
- `one_dark` - Atom's gift
- `catppuccin_mocha` - Smooth pastels
- `tokyo_night` - Neon dreams

## 🚀 Temporary VMs

Create lightweight VMs for experiments and code reviews without any configuration. Perfect for quick testing, exploring PRs, or trying new tools in isolation.

### ✨ Features

- **Zero configuration**: No vm.yaml needed
- **Modern syntax**: Space-separated directory mounting
- **Permission support**: Read-only (`:ro`) and read-write (`:rw`) access
- **Dynamic mount management**: Add/remove mounts without losing work
- **Full lifecycle control**: Start, stop, restart VMs
- **Command execution**: Run commands without SSH
- **State management**: Tracks temp VM state in `~/.vm/temp-vm.state`
- **Alias support**: Use `vm tmp` as shorthand for `vm temp`
- **Lightweight**: Basic Ubuntu container for quick experiments

### 🎯 Basic Usage

```bash
# Create temp VM with multiple mounts (modern syntax)
vm temp ./src ./tests ./docs:ro

# Or use the shorthand alias
vm tmp ./src ./tests ./config

# Mount with explicit permissions
vm temp ./src:rw ./config:ro ./tests:rw

# SSH into temp VM
vm temp ssh
vm temp ssh -c "npm test"  # Run command and exit

# Check temp VM status
vm temp status

# Destroy temp VM
vm temp destroy
```

### 🛠️ Dynamic Mount Management

Add and remove mounts from running temp VMs without losing your work:

```bash
# Start with basic mounts
vm temp ./src ./tests

# Add a new directory while working
vm temp mount ./new-feature
vm temp mount ./docs:ro

# Remove directories you no longer need
vm temp unmount ./old-code

# List current mounts  
vm temp mounts

# List active temp VMs
vm temp list

# Clean up and remove all mounts
vm temp unmount --all
```

### 🔄 Lifecycle Management

Full control over your temp VM lifecycle:

```bash
# Stop temp VM (preserves all data and state)
vm temp stop

# Start stopped temp VM
vm temp start

# Restart temp VM
vm temp restart

# View container logs
vm temp logs
vm temp logs -f  # Follow logs in real-time

# Re-run provisioning if needed
vm temp provision
```

### 🔄 Container Recreation

When adding or removing mounts, the temp VM automatically recreates the container while preserving your `/home/developer` directory:

```bash
# Start working
vm temp ./src ./tests
vm temp ssh  # Do some work, install packages, etc.

# Add new mount - container recreates but /home/developer preserved
vm temp mount ./docs
# 🔄 Recreating container with updated mounts...
# ✅ Container recreated with updated mounts in 5 seconds

# Your work and installed packages are still there!
```

### 💡 Use Cases

- **Quick testing**: Test libraries or configurations without affecting main project
- **Code reviews**: Safely explore PRs in isolation
- **Experiments**: Try new tools or configurations with full lifecycle control
- **Debugging**: Isolate issues with minimal setup
- **Iterative development**: Add/remove project directories as you work
- **Log monitoring**: Real-time log viewing during development
- **Command execution**: Run builds, tests, or scripts without SSH overhead

### ⚠️ Limitations

- **Docker only**: Temp VMs use Docker containers, not full Vagrant VMs
- **Basic environment**: No services (PostgreSQL, Redis, etc.) - just Ubuntu + basic tools
- **Home directory persistence**: Only `/home/developer` is preserved during mount changes
- **No custom configuration**: Uses built-in minimal setup

### 🔄 Backward Compatibility

The old comma-separated syntax still works but shows a deprecation warning:

```bash
# Old syntax (still works)
vm temp ./src,./tests,./docs:ro
# ⚠️  Warning: Comma-separated mounts are deprecated
#    Please use: vm temp ./src ./tests ./docs:ro

# New syntax (recommended)
vm temp ./src ./tests ./docs:ro
```

## 🧪 Docker vs Vagrant: Which to Choose?

**Both providers now offer identical development environments!** Services run on localhost, commands work the same, and Ansible handles all provisioning. The only differences are:

**Docker (Default - Container Isolation)**:
- ✅ Lightweight and fast
- ✅ Minimal resource usage (~500MB RAM)
- ✅ Quick startup/teardown (~10-30 seconds)
- ✅ Perfect for most development needs
- ❌ Shared kernel with host
- ❌ Less isolation for risky operations

**Vagrant (Full VM Isolation)**:
- ✅ Separate kernel = maximum security
- ✅ Perfect for `claude --dangerously-skip-permissions`
- ✅ Complete OS-level isolation
- ❌ Higher resource usage (~2GB RAM)
- ❌ Slower startup times (~2-3 minutes)

**The development experience is now identical**: Same commands, same localhost connections, same Ansible provisioning. Choose based on your security/performance needs.

## 💡 Tips & Tricks

### 🔄 File Sync

```
Mac: ~/your-project/src/app.js
 ↕️ (instant sync)
VM:  /workspace/src/app.js
```

### 🐘 Database Backups

Drop `.sql.gz` files matching your `backup_pattern` in the project - they'll auto-restore on provision!

### 🚪 Port Conflicts

See "port collision" in output? Vagrant auto-remapped it:

```
Fixed port collision for 3000 => 3000. Now on port 2200.
```

## 🚨 Troubleshooting

**Q: Port conflicts?**  
A: Check output for remapped ports (Vagrant) or adjust ports in vm.yaml

**Q: VM/container won't start?**  
A: `vm destroy` then `vm create`

**Q: Slow performance?**  
A: Increase memory/CPUs in vm.yaml (or switch to Docker provider)

**Q: Can't connect to service?**  
A: 
- Check service is enabled in vm.yaml
- Verify service is running: `vm exec 'systemctl status postgresql'`
- All services use localhost (not container names)

**Q: VirtualBox stuck?**  
A: `vm kill` to force cleanup

**Q: Provisioning failed?**  
A: Check Ansible output - it handles provisioning for both providers:
```bash
vm provision  # Re-run Ansible playbook
```

## 🏗️ Technical Architecture

### Unified Provisioning
Both Vagrant and Docker providers use the **same Ansible playbook** for provisioning. This ensures identical environments regardless of provider choice:

```
vm.sh → Provider (Vagrant/Docker) → Ansible Playbook → Configured Environment
```

### Service Architecture
All services (PostgreSQL, Redis, MongoDB) run **inside** the VM/container and are accessed via `localhost`. No more confusion about container hostnames vs localhost!

### Configuration Flow
1. `vm.yaml` defines your requirements
2. `validate-config.sh` merges with defaults and validates
3. Provider-specific setup (Vagrantfile or docker-compose.yml)
4. Ansible playbook provisions everything identically

---

**Pro tip**: The package includes `vm.yaml` with sensible defaults. Your project's `vm.yaml` only needs what's different! 🎪