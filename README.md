# 🚀 VM Infrastructure

Beautiful development environments with one command. Choose between Docker (lightweight containers, default) or Vagrant (full VM isolation) based on your needs.

> **🔐 Built for Claude Code**: This infrastructure provides safe sandboxes for AI-assisted development. Choose your isolation level:
> - **Docker (default)**: Lightweight containers with shared kernel - fast and resource-efficient for most workloads
> - **Vagrant**: Full VM isolation with separate kernel - ideal for `claude --dangerously-skip-permissions`

## 📚 Table of Contents

- [Quick Start](#-quick-start)
- [What's Included](#-whats-included)
- [Terminal Themes](#-terminal-themes)
- [Configuration](#-configuration)
    - [Minimal Setup](#-minimal-setup)
    - [Full Reference](#-full-reference)
    - [Terminal Options](#-terminal-options)
- [Commands](#-commands)
- [Port Strategy](#-port-strategy)
- [Tips & Tricks](#-tips--tricks)
- [Troubleshooting](#-troubleshooting)
- [Installation](#-installation)

## 🏃 Quick Start

### Option 1: npm Global Installation (Recommended)

```bash
# 1. Install globally via npm
npm install -g @goobits/vm

# 2. Start immediately with defaults OR create custom vm.json
vm create  # Works without any config! Uses smart defaults
vm ssh     # Enter your shiny new Ubuntu box

# OR customize with vm.json
{
  "ports": {
    "frontend": 3000,
    "backend": 3001
  }
  # Default provider is Docker - add "provider": "vagrant" for full VM
}
```

### Option 2: Manual Global Installation

```bash
# 1. Clone and install
git clone <repo-url>
cd vm
./install.sh

# 2. Use globally
vm create
```

### Option 3: Per-Project Installation

```bash
# 1. Copy to your project
cp -r vm your-project/

# 2. Add to package.json
{
  "scripts": {
    "vm": "./vm/vm.sh"
  }
}

# 3. Launch!
pnpm vm create
```

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
- **Automatic language installation**: Rust (via cargo_packages) and Python (via pip_packages)

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

## ⚙️ Configuration

📖 **Full configuration reference**: See [CONFIGURATION.md](CONFIGURATION.md) for all available options.

### 🎯 Minimal Setup

Most projects just need ports. Everything else has smart defaults:

```json
{
	"ports": {
		"frontend": 3020,
		"backend": 3022
	}
}
```

Want PostgreSQL? Just add:

```json
{
	"ports": {
		"frontend": 3020,
		"backend": 3022,
		"postgresql": 3025
	},
	"services": {
		"postgresql": { "enabled": true }
	}
}
```

### 🚀 Automatic Language Installation

Need Rust or Python? Just add packages and the VM automatically installs the language runtime:

```json
{
	"cargo_packages": ["cargo-watch", "tokei"],     // Installs Rust + Cargo
	"pip_packages": ["black", "pytest", "mypy"]     // Installs Python + pyenv
}
```

The VM will:
- **Rust**: Install via rustup with stable toolchain when `cargo_packages` is present
- **Python**: Install pyenv + Python 3.11 when `pip_packages` is present
- **Node.js**: Already included by default (configurable version)

### 📋 IDE Support

For autocompletion and validation in your editor:

```json
{
	"$schema": "./vm.schema.json",
	"ports": {
		"frontend": 3020
	}
}
```

### 📋 Full Reference

```json
{
	"provider": "docker", // or "vagrant" - defaults to "docker"
	"project": {
		"name": "my-app", // VM/container name & prompt
		"hostname": "dev.my-app.local", // VM/container hostname
		"workspace_path": "/workspace", // Sync path in VM/container
		"env_template_path": null, // e.g. "backend/.env.template"
		"backup_pattern": "*backup*.sql.gz" // For auto-restore
	},
	"vm": {
		"box": "bento/ubuntu-24.04", // Vagrant box (Vagrant only)
		"memory": 4096, // RAM in MB
		"cpus": 2, // CPU cores
		"user": "vagrant", // VM/container user
		"port_binding": "127.0.0.1" // or "0.0.0.0" for network
	},
	"versions": {
		"node": "22.11.0", // Node version
		"nvm": "v0.40.3", // NVM version
		"pnpm": "latest" // pnpm version
	},
	"ports": {
		"frontend": 3000,
		"backend": 3001,
		"postgresql": 5432,
		"redis": 6379
	},
	"services": {
		"postgresql": {
			"enabled": true,
			"database": "myapp_dev",
			"user": "postgres",
			"password": "postgres"
		},
		"redis": { "enabled": true },
		"mongodb": { "enabled": false },
		"docker": { "enabled": true },
		"headless_browser": { "enabled": false }
	},
	"npm_packages": [
		// Global npm packages
		"prettier",
		"eslint"
	],
	"cargo_packages": [
		// Global Cargo packages (triggers Rust installation)
		"cargo-watch",
		"tokei"
	],
	"pip_packages": [
		// Global pip packages (triggers Python/pyenv installation)
		"black",
		"pytest"
	],
	"aliases": {
		// Custom aliases
		"dev": "pnpm dev",
		"test": "pnpm test"
	},
	"environment": {
		// ENV vars
		"NODE_ENV": "development"
	}
}
```

### 🎭 Terminal Options

Make your prompt uniquely yours:

```json
{
	"terminal": {
		"emoji": "⚡", // Prompt emoji
		"username": "hacker", // Prompt name
		"theme": "tokyo_night", // Color theme
		"show_git_branch": true, // Show branch
		"show_timestamp": false // Show time
	}
}
```

Result: `⚡ hacker my-app (main) >`

## 🎮 Commands

```bash
vm init                      # Initialize a new vm.json configuration file
vm generate                  # Generate vm.json by composing services
vm list                      # List all VM instances
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

## 🔍 Automatic vm.json Discovery

The `vm` command automatically searches for `vm.json` configuration:

1. **Current directory**: `./vm.json`
2. **Parent directory**: `../vm.json`
3. **Grandparent directory**: `../../vm.json`
4. **Defaults**: If no config found, uses built-in defaults

This means you can run `vm create` from anywhere in your project tree, and it will find the right configuration!

## 🔌 Port Strategy

Avoid conflicts by giving each project 10 ports:

- **Project 1**: 3000-3009
- **Project 2**: 3010-3019
- **Project 3**: 3020-3029
- **Project 4**: 3030-3039

Example allocation:

```json
{
	"ports": {
		"frontend": 3020, // Main app
		"backend": 3022, // API
		"postgresql": 3025, // Database
		"redis": 3026, // Cache
		"docs": 3028 // Documentation
	}
}
```

**Network access?** Add `"port_binding": "0.0.0.0"` to share with your network.

## 🔄 Advanced Features

**Aliases**: Custom shell aliases from `vm.json` are applied via Ansible. To update without reprovisioning: edit `~/.zshrc` manually or run `vm provision`.

**Claude Sync**: Add `"claude_sync": true` to sync Claude AI data to `~/.claude/vms/{project_name}/` on your host.

**Database Persistence**: Add `"persist_databases": true` to store database data in `.vm/data/` (survives VM rebuilds). Add `.vm/` to `.gitignore`.

## 💡 Tips & Tricks

### 🔄 File Sync

```
Mac: ~/your-project/src/app.js
 ↕️ (instant sync)
VM:  /workspace/src/app.js
```

### 🧪 Docker vs Vagrant: Which to Choose?

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

### 🐘 Database Backups

Drop `.sql.gz` files matching your `backup_pattern` in the project - they'll auto-restore on provision!

### 🚪 Port Conflicts

See "port collision" in output? Vagrant auto-remapped it:

```
Fixed port collision for 3000 => 3000. Now on port 2200.
```

## 🚨 Troubleshooting

**Q: Port conflicts?**  
A: Check output for remapped ports (Vagrant) or adjust ports in vm.json

**Q: VM/container won't start?**  
A: `vm destroy` then `vm create`

**Q: Slow performance?**  
A: Increase memory/CPUs in vm.json (or switch to Docker provider)

**Q: Can't connect to service?**  
A: 
- Check service is enabled in vm.json
- Verify service is running: `vm exec 'systemctl status postgresql'`
- All services use localhost (not container names)

**Q: VirtualBox stuck?**  
A: `vm kill` to force cleanup

**Q: Provisioning failed?**  
A: Check Ansible output - it handles provisioning for both providers:
```bash
vm provision  # Re-run Ansible playbook
```

## 💻 Installation

### Prerequisites

**For Vagrant provider**:
- VirtualBox or Parallels
- Vagrant

**For Docker provider**:
- Docker Desktop (macOS/Windows) or Docker Engine (Linux)
- docker-compose

### macOS

```bash
# For Vagrant
brew tap hashicorp/tap
brew install hashicorp/tap/hashicorp-vagrant
brew install --cask virtualbox

# For Docker
brew install --cask docker
```

### Ubuntu/Debian

```bash
# For Vagrant
wget -O- https://apt.releases.hashicorp.com/gpg | \
  sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] \
  https://apt.releases.hashicorp.com $(lsb_release -cs) main" | \
  sudo tee /etc/apt/sources.list.d/hashicorp.list
sudo apt update && sudo apt install vagrant virtualbox

# For Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
```

### Windows

**Vagrant**: Download from [vagrant.com](https://www.vagrantup.com/downloads) and [virtualbox.org](https://www.virtualbox.org/wiki/Downloads)
**Docker**: Install [Docker Desktop](https://www.docker.com/products/docker-desktop)

## 🏗️ Technical Architecture

### Unified Provisioning
Both Vagrant and Docker providers use the **same Ansible playbook** for provisioning. This ensures identical environments regardless of provider choice:

```
vm.sh → Provider (Vagrant/Docker) → Ansible Playbook → Configured Environment
```

### Service Architecture
All services (PostgreSQL, Redis, MongoDB) run **inside** the VM/container and are accessed via `localhost`. No more confusion about container hostnames vs localhost!

### Configuration Flow
1. `vm.json` defines your requirements
2. `validate-config.sh` merges with defaults and validates
3. Provider-specific setup (Vagrantfile or docker-compose.yml)
4. Ansible playbook provisions everything identically

---

**Pro tip**: The package includes `vm.json` with sensible defaults. Your project's `vm.json` only needs what's different! 🎪
