# 🚀 Goobits VM Infrastructure

Beautiful development environments with one command. Choose between Docker (lightweight containers, default) or Vagrant (full VM isolation) based on your needs.

> **🔐 Built for AI Agents**: This infrastructure provides safe sandboxes for AI-assisted development when you need system isolation. Choose your isolation level:
> - **Docker (default)**: Lightweight containers with shared kernel - fast and resource-efficient for most workloads
> - **Vagrant**: Full VM isolation with separate kernel - ideal for risky operations or when system security is a concern

## 🏃 Quick Start

```bash
# Install globally via npm
npm install -g @goobits/vm

# Start immediately with defaults
vm create  # Works without any config! Uses smart defaults
vm ssh     # Enter your shiny new Ubuntu box
```

📖 **Need help installing?** See the complete [Installation Guide](INSTALLATION.md) for all installation options and troubleshooting.

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

# Use custom config file
vm --config prod.json create # Create with specific config
vm --config dev.json ssh     # Any command works with --config
```

## 📚 Documentation

- 📖 [Installation Guide](INSTALLATION.md) - Complete installation instructions for all platforms
- ⚙️ [Configuration Reference](CONFIGURATION.md) - Full configuration options and examples
- 🔄 [Changelog](CHANGELOG.md) - Recent updates and version history

## 📦 What's Included

- **Ubuntu 24.04 LTS** with Zsh + syntax highlighting
- **Node.js v22** via NVM (configurable)
- **pnpm** via Corepack
- **Beautiful terminals** with 8 themes
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

Need a quick, disposable environment for testing or experimentation? Use the temp VM feature - perfect for one-off tasks without creating a full vm.yaml configuration.

### ✨ Features

- **Zero configuration**: No vm.yaml needed
- **Selective directory mounting**: Choose exactly which folders to include
- **State management**: Tracks temp VM state in `~/.vm/temp-vm.state`
- **Smart collision handling**: Intelligently manages conflicts when mounts change
- **Dedicated subcommands**: `ssh`, `status`, `destroy` for better control
- **Alias support**: Use `vm tmp` as shorthand for `vm temp`
- **Lightweight**: Basic Ubuntu container for quick experiments

### 🎯 Usage

```bash
# Create temp VM with specific directories
vm temp ./src,./tests,./config

# Or use the shorthand alias
vm tmp ./src,./tests,./config

# Mount with permissions (read-only/read-write)
vm temp ./src:rw,./docs:ro,./tests

# SSH into temp VM directly
vm temp ssh
vm temp ssh -c "npm test"  # Run command and exit

# Check temp VM status
vm temp status

# Destroy temp VM
vm temp destroy
```

### 🔄 Smart Collision Handling

The temp VM now intelligently handles conflicts when you try to create a new temp VM with different mounts:

1. **Same mounts**: Automatically connects to existing temp VM
2. **Different mounts**: Prompts you with options:
   - **Connect anyway**: Use existing VM (mounts won't match)
   - **Recreate**: Destroy old VM and create new one with correct mounts
   - **Cancel**: Abort the operation

```bash
# First time - creates new temp VM
vm temp ./client,./server

# Same command - connects to existing temp VM
vm temp ./client,./server

# Different mounts - prompts for action
vm temp ./frontend,./backend
# > Temp VM exists with different mounts. What would you like to do?
# > 1) Connect anyway  2) Recreate  3) Cancel
```

### 💡 Use Cases

- **Quick testing**: Test libraries or configurations without affecting main project
- **Code reviews**: Safely explore PRs in isolation
- **Experiments**: Try new tools or configurations
- **Debugging**: Isolate issues with minimal setup
- **Temporary work**: One-off tasks that don't need persistent environments

### ⚠️ Limitations

- **Docker only**: Temp VMs use Docker containers, not full Vagrant VMs
- **Basic environment**: No services (PostgreSQL, Redis, etc.) - just Ubuntu + basic tools
- **No persistence**: Data is lost when temp VM is destroyed
- **No custom configuration**: Uses built-in minimal setup

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