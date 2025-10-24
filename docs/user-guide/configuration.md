# ⚙️ Configuration Guide

Complete reference for configuring your VM development environment with YAML.

## 📖 Table of Contents

- [Configuration Files](#-configuration-files)
- [Quick Start](#-quick-start)
- [Global Services Configuration](#-global-services-configuration)
- [Full Reference](#-full-reference)
- [Services](#-services)
- [Language Runtimes](#-language-runtimes)
- [Terminal Customization](#-terminal-customization)
- [Advanced Features](#-advanced-features)

For configuration examples, see the [Examples Guide](../getting-started/examples.md).

## 📁 Configuration Files

The VM tool uses two types of configuration files:

### VM-Specific Configuration (`vm.yaml`)
Located in your project directory, this configures VM-specific settings:
- **Project settings** (name, ports, workspace path)
- **VM resources** (memory, CPUs, operating system)
- **VM-specific services** (PostgreSQL, Redis, MongoDB for this VM)
- **Development environment** (packages, aliases, versions)

```yaml
# vm.yaml - Controls THIS project's VM
os: ubuntu
project:
  name: my-project
ports:
  frontend: 3000
services:
  postgresql:
    enabled: true
```

### Global Configuration (`~/.vm/config.yaml`)
Located in your home directory, this configures system-wide settings:
- **Global services** (Docker registry, auth proxy, package registry)
- **Default values** for new VMs (provider, memory, terminal settings)
- **Feature flags** and user preferences

```yaml
# ~/.vm/config.yaml - Controls global settings for ALL VMs
services:
  docker_registry:
    enabled: true
    max_cache_size_gb: 10
defaults:
  provider: docker
  memory: 4096
```

**Key Difference:** VM config controls individual project services, global config controls shared infrastructure services.

## 🚀 Quick Start

### Simplest Setup (Recommended)

Just specify your OS - everything else is auto-configured:

```yaml
# Minimal setup with defaults
os: ubuntu
provider: docker
project:
  name: my-project
```

### Add Ports

Need specific ports? Just add them:

```yaml
os: ubuntu
provider: docker
project:
  name: my-project
ports:
  frontend: 3020
  backend: 3022
```

### Add Services

Want PostgreSQL? Just enable it:

```yaml
os: ubuntu
provider: docker
project:
  name: my-project
ports:
  backend: 3022
  postgresql: 3025
services:
  postgresql:
    enabled: true
```

### Advanced: Explicit Provider

When you need specific provider features:

```yaml
provider: docker  # Force Docker provider
vm:
  memory: "8gb"  # Supports: 8192 (MB), "8gb", "50%", "unlimited"
  cpus: 4        # Supports: 4, "50%", "unlimited"
```

## 📁 Configuration Files

### Automatic Discovery

The `vm` command automatically searches for `vm.yaml` configuration:

1. **Current directory**: `./vm.yaml`
2. **Parent directory**: `../vm.yaml`
3. **Grandparent directory**: `../../vm.yaml`
4. **Auto defaults with presets**: If no config found, uses built-in defaults enhanced by automatic preset detection

This means you can run `vm create` from anywhere in your project tree! The tool will also analyze your project files to automatically apply appropriate presets.

### IDE Support

For autocompletion and validation in your editor:

```yaml
# yaml-language-server: $schema=../../configs/schema/vm.schema.yaml
ports:
  frontend: 3020
```

## 📋 Full Reference

```yaml
version: "1.0"  # Configuration format version

# Simple mode (recommended)
os: ubuntu  # Options: ubuntu, macos, debian, alpine, linux, auto
           # Provider auto-selected based on your platform
           # 'auto' detects OS from your project files

# Advanced mode (when you need specific control)
provider: docker  # Options: docker, vagrant, tart
                 # Note: Use 'os' field for simpler setup

project:
  name: my-app  # VM/container name & prompt
  hostname: dev.my-app.local  # VM/container hostname (required)
  workspace_path: /workspace  # Sync path in VM/container
  env_template_path: null  # e.g. "backend/.env.template"
  backup_pattern: "*backup*.sql.gz"  # For auto-restore

vm:
  box: bento/ubuntu-24.04  # Vagrant box (Vagrant only)
  memory: 4096  # RAM: 4096 (MB), "2gb", "50%", "unlimited"
  cpus: 2  # CPUs: 2, "50%", "unlimited"
  swap: 2048 # Swap: 2048 (MB), "1gb", "50%", "unlimited"
  swappiness: 60 # Swappiness (0-100)
  user: developer  # VM/container user (changed from vagrant)
  port_binding: 127.0.0.1  # or "0.0.0.0" for network access

versions:
  node: 22.11.0  # Node version
  nvm: v0.40.3  # NVM version
  pnpm: latest  # pnpm version

ports:
  frontend: 3000
  backend: 3001
  postgresql: 5432
  redis: 6379

services:
  postgresql:
    enabled: true
    database: myapp_dev
    user: postgres
    password: postgres
  redis:
    enabled: true
  mongodb:
    enabled: false
  docker:
    enabled: true
  headless_browser:
    enabled: false

npm_packages:
  # Global npm packages
  - prettier
  - eslint

cargo_packages:
  # Global Cargo packages (triggers Rust installation)
  - cargo-watch
  - tokei

pip_packages:
  # Global pip packages (triggers Python/pyenv installation)
  - black
  - pytest

aliases:
  # Custom aliases
  dev: pnpm dev
  test: pnpm test

environment:
  # Environment variables
  NODE_ENV: development
  API_URL: http://localhost:3001

terminal:
  emoji: "⚡"  # Prompt emoji
  username: hacker  # Prompt name
  theme: tokyo_night  # Color theme
  show_git_branch: true  # Show branch
  show_timestamp: false  # Show time
```

## 🛠️ Services

VM-scoped services are configured in `vm.yaml` and run **inside** each VM/container. Global services are configured in `~/.vm/config.yaml` and shared across all VMs. All services are accessed via `localhost` from within the VM.

### PostgreSQL

```yaml
# vm.yaml - VM-scoped service
services:
  postgresql:
    enabled: true
    database: myapp_dev
    user: postgres
    password: postgres
ports:
  postgresql: 5432  # Access via localhost:5432
```

### Redis

```yaml
# vm.yaml - VM-scoped service
services:
  redis:
    enabled: true
ports:
  redis: 6379  # Access via localhost:6379
```

### MongoDB

```yaml
# vm.yaml - VM-scoped service
services:
  mongodb:
    enabled: true
ports:
  mongodb: 27017  # Access via localhost:27017
```

### MySQL

```yaml
# vm.yaml - VM-scoped service
services:
  mysql:
    enabled: true
ports:
  mysql: 3306  # Access via localhost:3306
```

### Docker-in-Docker

```yaml
# vm.yaml - VM-scoped service
services:
  docker:
    enabled: true  # Allows running docker commands inside VM
```

### Headless Browser (for testing)

```yaml
# vm.yaml - VM-scoped service
services:
  headless_browser:
    enabled: true  # Installs Chrome/Chromium for testing
```

## 🌐 Global Services Configuration

Global services are configured in `~/.vm/config.yaml` and serve **all** VMs on your system.

### Service Architecture Overview

The VM tool supports two types of services:

#### VM-Scoped Services (configured in `vm.yaml`)
Each VM gets its own instance of these services:
- **postgresql** - Database per VM
- **redis** - Cache per VM
- **mongodb** - Database per VM
- **mysql** - Database per VM
- **docker** - Docker-in-Docker per VM
- **headless_browser** - Browser testing per VM
- **audio** - Audio support per VM
- **gpu** - GPU acceleration per VM

#### Global Services (configured in `~/.vm/config.yaml`)
Shared across all VMs with automatic lifecycle management:
- **docker_registry** - Docker image caching and registry mirror
- **auth_proxy** - Authentication proxy for secure secret management
- **package_registry** - Package caching for npm, pip, and cargo

### Global Service Lifecycle

Global services use reference counting:
1. **Auto-start** when first VM needs them
2. **Auto-stop** when last VM stops using them
3. **Zero maintenance** - fully automated lifecycle
4. **Shared resources** - all VMs benefit from the same service instance

### Docker Registry (Automatic Caching) 🆕

Enable intelligent Docker image caching that works like a browser cache - completely invisible while dramatically speeding up Docker pulls:

```yaml
# ~/.vm/config.yaml - Global configuration
services:
  docker_registry:
    enabled: true  # That's it! Zero-configuration caching
```

**What it does:**
- **Auto-starts** when any VM needs it
- **Caches all Docker images** locally for instant pulls
- **Self-manages** with automatic cleanup of old images
- **Auto-configures** Docker daemon to use local mirror
- **Stops automatically** when no VMs need it

**Advanced configuration** (optional):
```yaml
# ~/.vm/config.yaml
services:
  docker_registry:
    enabled: true
    max_cache_size_gb: 10        # Max cache size (default: 5GB)
    max_image_age_days: 60       # Keep images for 60 days (default: 30)
    cleanup_interval_hours: 2    # Cleanup frequency (default: 1 hour)
    enable_lru_eviction: true     # LRU when cache full (default: true)
    enable_auto_restart: true     # Auto-restart on failure (default: true)
    health_check_interval_minutes: 30  # Health check interval (default: 15)
```

**Benefits:**
- 🚀 **10-100x faster** Docker pulls after first cache
- 💾 **Bandwidth savings** - images pulled once, used many times
- 🤖 **Zero maintenance** - automatic cleanup and management
- 🔄 **Shared cache** - all VMs share the same image cache

### Auth Proxy (Secure Secrets) 🔐

Enable secure secret management across all VMs:

```yaml
# ~/.vm/config.yaml
services:
  auth_proxy:
    enabled: true
    port: 3090              # Port for auth proxy (default: 3090)
    token_expiry_hours: 24  # Token expiry (default: 24)
```

### Package Registry (Shared Cache) 📦

Enable shared package caching for npm, pip, and cargo:

```yaml
# ~/.vm/config.yaml
services:
  package_registry:
    enabled: true
    port: 3080           # Port for package registry (default: 3080)
    max_storage_gb: 10   # Max storage size (default: 10GB)
```

### Managing Global Services

#### Check Service Status
```bash
# View all service status
vm config get services --global

# Check specific service
vm config get services.docker_registry --global
```

#### Enable/Disable Services
```bash
# Enable Docker registry
vm config set services.docker_registry.enabled true --global

# Disable auth proxy
vm config set services.auth_proxy.enabled false --global

# Configure service settings
vm config set services.docker_registry.max_cache_size_gb 20 --global
```

#### Service Commands
```bash
# Package registry management
vm pkg status              # Check package registry status
vm pkg list               # List cached packages

# Auth proxy management
vm auth status            # Check auth proxy status
vm auth list              # List stored secrets
```

#### Service Logs and Debugging
Global services run in the background and log to the system journal. To debug issues:

```bash
# Check if services are running
docker ps | grep vm-registry   # Docker registry containers
curl http://localhost:3080/health  # Package registry health
curl http://localhost:3090/health  # Auth proxy health

# View service logs
docker logs vm-registry-proxy      # Docker registry logs
docker logs vm-package-server      # Package registry logs
```


## 🖥️ Operating System Configuration

### OS Field (Recommended)

The `os` field provides automatic provider selection and optimized settings:

```yaml
os: ubuntu   # Docker/Vagrant, 4GB RAM, full dev stack
os: macos    # Tart on Apple Silicon, 8GB RAM
os: debian   # Docker/Vagrant, 2GB RAM, lightweight
os: alpine   # Docker only, 1GB RAM, minimal
os: linux    # Docker/Vagrant, 4GB RAM, generic Linux
os: auto     # Auto-detect from project files
```

**Note**: OS-specific settings override the schema defaults (2GB RAM, 2 CPUs)

**Auto-detection**: The system automatically selects the best provider:
- **Apple Silicon Mac + `os: macos`** → Tart provider
- **Apple Silicon Mac + `os: ubuntu`** → Docker provider
- **Intel/AMD + any OS** → Docker or Vagrant based on availability

### Tart Provider (Apple Silicon)

Native virtualization for Apple Silicon Macs:

```yaml
# Automatic with OS field
os: macos  # Automatically uses Tart on M1/M2/M3

# Or explicit configuration
provider: tart
tart:
  image: ghcr.io/cirruslabs/macos-sonoma-base:latest
  rosetta: true  # Enable x86 emulation for Linux VMs
  disk_size: 60  # Disk: 60 (GB), "60gb", "50%"
  ssh_user: admin
```

**Requirements**: Apple Silicon Mac (M1/M2/M3/M4), Tart installed via `brew install cirruslabs/cli/tart`

## 🗣️ Language Runtimes

### Automatic Installation

Languages are automatically installed when you specify packages:

```yaml
cargo_packages: ["cargo-watch", "tokei"]     # Installs Rust + Cargo
pip_packages: ["black", "pytest", "mypy"]     # Installs Python + pyenv
npm_packages: ["prettier", "eslint"]          # Node.js included by default
```

The VM will:
- **Rust**: Install via rustup with stable toolchain when `cargo_packages` is present
- **Python**: Install pyenv + Python 3.11 when `pip_packages` is present
- **Node.js**: Already included by default (configurable version)

### Version Control

```yaml
versions:
  node: 22.11.0    # Specific Node.js version
  nvm: v0.40.3     # NVM version
  pnpm: latest     # pnpm version
```

## 🎨 Terminal Customization

### Available Themes

- `dracula` ⭐ - Purple magic (default)
- `gruvbox_dark` - Retro warmth
- `solarized_dark` - Science-backed colors
- `nord` - Arctic vibes
- `monokai` - Classic vibrance
- `one_dark` - Atom's gift
- `catppuccin_mocha` - Smooth pastels
- `tokyo_night` - Neon dreams

### Custom Prompt

```yaml
terminal:
  emoji: "🚀"           # Prompt emoji
  username: developer   # Prompt name
  theme: dracula       # Color theme
  show_git_branch: true    # Show git branch
  show_timestamp: false   # Show timestamp
```

Result: `🚀 developer my-app (main) >`

## 🔌 Port Strategy

Avoid conflicts by giving each project 10 ports:

- **Project 1**: 3000-3009
- **Project 2**: 3010-3019
- **Project 3**: 3020-3029
- **Project 4**: 3030-3039

Example allocation:

```yaml
ports:
  frontend: 3020    # Main app
  backend: 3022     # API
  postgresql: 3025  # Database
  redis: 3026       # Cache
  docs: 3028        # Documentation
```

### Network Access

**Local only (default):**
```yaml
vm:
  port_binding: 127.0.0.1
```

**Network accessible:**
```yaml
vm:
  port_binding: "0.0.0.0"  # Share with your network
```

## 🔄 Advanced Features

### Git Worktrees (New in 2.0.6)

Enable Git worktree support for multi-branch development:

**Global Configuration** (`~/.vm/config.yaml`):
```yaml
worktrees:
  enabled: true
  base_path: ~/worktrees  # Optional: custom worktree location
```

**Project Configuration** (`vm.yaml`):
```yaml
worktrees:
  enabled: true  # Override global setting per-project
```

**Features**:
- **Automatic mounting** at identical absolute paths (host and container)
- **Create worktrees from inside containers** - they work on host too!
- **Helper command** `vm-worktree` for easy management
- **Prompt indicator** shows current worktree
- Automatic detection and remounting of existing worktrees
- Support for multiple branches simultaneously

**Use Cases**:
- Developing multiple branches simultaneously
- Testing feature branches in isolation
- Quick branch switching without leaving container
- CI/CD workflows with parallel branch testing

**Example Workflows**:

**Creating worktrees from inside container (NEW!):**
```bash
vm ssh

# Create worktree from inside container
vm-worktree add feature-x
# ✓ Worktree created: feature-x
# (worktree:feature-x) $  # Automatically navigated with prompt indicator

# Work on the feature
git commit -m "Add feature"

# On host (also works!)
cd ~/.vm/worktrees/myproject/feature-x
git status  # Shows your commits

# List all worktrees
vm-worktree list
# 📁 Worktrees:
#   feature-x
#   bugfix-123

# Jump to another worktree
vm-worktree goto bugfix-123
(worktree:bugfix-123) $

# Remove when done
vm-worktree remove feature-x
# ✓ Worktree removed: feature-x
```

**Traditional workflow (still supported):**
```bash
# Create worktree on host
git worktree add ../feature-branch

# Navigate and create VM
cd ../feature-branch
vm create

# Auto-detected and mounted
vm ssh  # Will detect and offer to mount new worktree
```

**Security Features**:
- **Path validation**: Prevents worktrees from escaping the designated directory
- **Input sanitization**: Worktree names are sanitized to prevent command injection
- **Safe directory mounting**: VM_WORKTREES can only point to safe, user-owned directories
- **Automatic discovery**: First-time users see helpful tips on `vm ssh`

**Shell History Persistence**:
Your command history is now preserved across container recreations! Both bash and zsh history are stored in a persistent Docker volume, so you won't lose your command history when rebuilding or recreating containers.

### Environment Templates

```yaml
project:
  env_template_path: "backend/.env.template"
```
- Automatically copies template to `.env` in VM

### Database Backups

**Automatic Backups on Destroy (Default)**

To prevent data loss, database services are now backed up automatically when you run `vm destroy`. This is the new default behavior.

You can disable this per-service in your `vm.yaml`:
```yaml
services:
  postgresql:
    backup_on_destroy: false # Disable auto-backup for this service
```

Or disable it for a single destroy command:
```bash
vm destroy --no-backup
```

**Global Backup Configuration**

You can configure backup settings globally in `~/.vm/config.yaml`:
```yaml
backups:
  enabled: true              # Global toggle for backups
  path: ~/.vm/backups        # Where to store backups
  keep_count: 5              # Number of backups to keep per service
  databases_only: true       # Only backup services of type 'database'
```

### Host System Integration

The VM tool can automatically inherit useful host system configuration to streamline your development workflow.

#### Git Configuration

Automatically copy your host's Git configuration (`user.name`, `user.email`, etc.) to the VM, so you can start making commits right away.

```yaml
# vm.yaml
copy_git_config: true  # default
```

To disable this feature, set `copy_git_config` to `false`:

```yaml
# vm.yaml
copy_git_config: false
```

#### Timezone

Automatically detect and set the VM's timezone to match your host system.

```yaml
# vm.yaml
vm:
  timezone: auto  # default
```

To override the timezone, specify a valid timezone name:

```yaml
# vm.yaml
vm:
  timezone: "America/New_York"
```

### Development Configuration

Enhanced developer workflows for SSH keys, dotfiles, and debugging support.

#### SSH Agent Forwarding 🔑

Securely use your host's SSH keys inside the VM without copying private keys:

```yaml
# vm.yaml
development:
  ssh_agent_forwarding: true  # Enable SSH agent forwarding
  mount_ssh_config: true      # Mount ~/.ssh/config (optional, defaults to true)
```

**What it does:**
- Forwards your host's SSH agent socket into the VM (read-only)
- Mounts `~/.ssh/config` for host alias support
- Enables git operations with SSH keys without exposing private keys
- Works with GitHub, GitLab, and other SSH-based services

**Requirements:**
- SSH agent running on host: `ssh-add -l` should list keys
- `SSH_AUTH_SOCK` environment variable set on host

**Usage inside VM:**
```bash
vm ssh
# Your SSH keys are now available
ssh -T git@github.com
git clone git@github.com:user/repo.git
```

**Security:**
- Private keys **never** copied to the VM
- Socket mounted read-only
- SSH config mounted read-only
- Agent forwarding is opt-in per project

**Troubleshooting:**
```bash
# Check if SSH agent is running on host
ssh-add -l

# If no agent running, start one
eval "$(ssh-agent -s)"
ssh-add ~/.ssh/id_ed25519

# Inside VM, verify SSH_AUTH_SOCK
echo $SSH_AUTH_SOCK  # Should be: /ssh-agent
ssh-add -l           # Should list your keys
```

## 📚 Additional Resources

- **[Examples Guide](../getting-started/examples.md)** - Real-world configuration examples
- **[Schema Reference](../api/configuration-schema.md)** - Complete field documentation
- **[Troubleshooting](troubleshooting.md)** - Common configuration issues