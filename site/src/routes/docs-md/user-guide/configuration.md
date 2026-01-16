# Configuration Guide

Complete reference for configuring your VM development environment with YAML.

> Migration Notice: The `examples/` directory workflow has been replaced by VM snapshots. See [VM Snapshots](#vm-snapshots) for the modern approach. Run `vm --version` to check your current version.

## Quick Reference

| Configuration | Location | Purpose |
|---------------|----------|---------|
| `vm.yaml` | Project root | VM-specific: services, ports, packages, environment |
| `~/.vm/config.yaml` | Home directory | Global: defaults, shared services, preferences |

**Most common fields:**
```yaml
# vm.yaml
project:
  name: my-app
os: ubuntu                    # ubuntu, macos, alpine
ports:
  app: 3000
services:
  postgresql:
    enabled: true
    database: myapp_dev
  redis:
    enabled: true
npm_packages:
  - typescript
  - eslint
```

**Key sections:**
- [Services](#services) - PostgreSQL, Redis, MongoDB, MySQL, Docker
- [Language Runtimes](#language-runtimes) - Node, Python, Rust versions
- [Host Sync](#host-system-integration) - SSH keys, dotfiles, AI tools
- [Advanced Features](#advanced-features) - Worktrees, snapshots

---

## Table of Contents

- [Configuration Files](#configuration-files)
- [Quick Start](#-quick-start)
- [Global Services Configuration](#-global-services-configuration)
- [Full Reference](#-full-reference)
- [Services](#-services)
- [Language Runtimes](#-language-runtimes)
- [Terminal Customization](#-terminal-customization)
- [Advanced Features](#-advanced-features)

For configuration examples, see the [Examples Guide](../getting-started/examples.md).

## Configuration Files

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

## Quick Start

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

## Configuration Files

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

## Full Reference

Complete field reference organized by category.

### Configuration Options Table

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| **Project** ||||
| `project.name` | string | directory name | VM/container name and shell prompt |
| `project.hostname` | string | required | VM/container hostname |
| `project.workspace_path` | string | /workspace | Mount path inside container |
| `project.env_template_path` | string | null | Optional template for .env tooling |
| `project.backup_pattern` | string | *backup*.sql.gz | Pattern for auto-restore |
| **VM Resources** ||||
| `vm.box` | string | ubuntu:24.04 | Base image (Docker/OCI) or box (Vagrant) |
| `vm.memory` | int/string | 4096 | RAM in MB, or "2gb", "50%", "unlimited" |
| `vm.cpus` | int/string | 2 | CPU cores, or "50%", "unlimited" |
| `vm.swap` | int/string | 2048 | Swap in MB, or "1gb", "50%", "unlimited" |
| `vm.swappiness` | int | 60 | Kernel swappiness (0-100) |
| `vm.user` | string | developer | Username inside container |
| `vm.port_binding` | string | 127.0.0.1 | Bind address ("0.0.0.0" for network) |
| **Operating System** ||||
| `os` | string | ubuntu | ubuntu, macos, debian, alpine, linux, auto |
| `provider` | string | auto | docker, vagrant, tart (auto-detected from OS) |
| **Language Versions** ||||
| `versions.node` | string | latest | Node.js version |
| `versions.npm` | string | auto | npm version (usually auto-installed with Node) |
| `versions.nvm` | string | latest | NVM version |
| `versions.pnpm` | string | latest | pnpm version |
| **Services** ||||
| `services.<name>.enabled` | bool | false | Enable service |
| `services.<name>.version` | string | latest | Service version |
| `services.<name>.database` | string | - | Database name (databases only) |
| `services.<name>.user` | string | - | Username (databases only) |
| `services.<name>.password` | string | - | Password (databases only) |
| **Development** ||||
| `npm_packages` | array | [] | Global npm packages to install |
| `cargo_packages` | array | [] | Global Cargo packages (installs Rust) |
| `pip_packages` | array | [] | Global pip packages (installs Python) |
| `aliases` | object | {} | Shell aliases (key: command) |
| `environment` | object | {} | Environment variables |
| **Terminal** ||||
| `terminal.emoji` | string | ⚡ | Shell prompt emoji |
| `terminal.username` | string | developer | Shell prompt username |
| `terminal.theme` | string | dracula | Color theme |
| `terminal.show_git_branch` | bool | true | Show git branch in prompt |
| `terminal.show_timestamp` | bool | false | Show timestamp in prompt |

---

### Project Configuration

```yaml
version: "2.0"

project:
  name: my-app              # VM name and shell prompt
  hostname: dev.local       # Container hostname (required)
  workspace_path: /workspace
  env_template_path: .env.template  # Optional template for .env tooling
```

### VM Resources

```yaml
vm:
  box: ubuntu:24.04         # Docker image or Vagrant box
  memory: 4096              # RAM: 4096 (MB), "2gb", "50%", "unlimited"
  cpus: 2                   # CPUs: 2, "50%", "unlimited"
  swap: 2048                # Swap memory in MB
  user: developer           # Username inside container
  port_binding: 127.0.0.1   # "0.0.0.0" for network access
```

### Operating System Selection

```yaml
# Simple mode (recommended)
os: ubuntu                  # ubuntu, macos, debian, alpine, auto

# Advanced mode (explicit provider)
provider: docker            # docker, vagrant, tart
```

### Language Versions

```yaml
versions:
  node: 22.11.0             # Specific Node.js version
  nvm: v0.40.3              # NVM version
  pnpm: latest              # pnpm package manager
```

### Port Configuration

```yaml
ports:
  frontend: 3000
  backend: 3001
  postgresql: 5432
  redis: 6379
```

### Service Configuration

```yaml
services:
  postgresql:
    enabled: true
    database: myapp_dev
    user: postgres
    password: postgres
  redis:
    enabled: true
  docker:
    enabled: true           # Docker-in-Docker
```

### Development Environment

> **Pre-installed packages**: The VM includes these npm packages by default: `@anthropic-ai/claude-code`, `@google/gemini-cli`, `npm-check-updates`, `prettier`, and `eslint`. Add additional packages below, or set `npm_packages: []` to start with a minimal environment.

```yaml
npm_packages:
  - typescript  # Adds to defaults
  - your-package

pip_packages:
  - black
  - pytest

aliases:
  dev: pnpm dev
  test: pnpm test

environment:
  NODE_ENV: development
  API_URL: http://localhost:3001

terminal:
  emoji: "⚡"
  username: hacker
  theme: tokyo_night
  show_git_branch: true
```

---

## Services

Configure services in `vm.yaml` (VM-scoped) or `~/.vm/config.yaml` (global, shared across VMs).

### VM-Scoped Services

Each VM gets its own instance. Access via `localhost` inside the VM.

| Service | Default Port | Common Options | Use Case |
|---------|--------------|----------------|----------|
| `postgresql` | 5432 | database, user, password, version | SQL database |
| `mysql` | 3306 | database, user, password, version | SQL database |
| `mongodb` | 27017 | database, user, password, version | NoSQL database |
| `redis` | 6379 | version | Cache / message broker |
| `docker` | - | buildx, driver | Docker-in-Docker for builds |
| `headless_browser` | - | display, executable_path | Playwright/Selenium testing |

**Common configurations:**

:::tabs

== PostgreSQL + Redis

```yaml
# vm.yaml - Most common: API with database and cache
services:
  postgresql:
    enabled: true
    database: myapp_dev
    user: postgres
    password: postgres
  redis:
    enabled: true

ports:
  postgresql: 5432
  redis: 6379
```

Connect to PostgreSQL at `localhost:5432` and Redis at `localhost:6379` inside your VM.

== PostgreSQL Only

```yaml
# vm.yaml - Simple database-backed app
services:
  postgresql:
    enabled: true
    database: myapp_dev
    user: postgres
    password: postgres

ports:
  postgresql: 5432
```

== All Databases

```yaml
# vm.yaml - Multi-database development
services:
  postgresql:
    enabled: true
    database: pg_dev
  mongodb:
    enabled: true
    database: mongo_dev
  redis:
    enabled: true

ports:
  postgresql: 5432
  mongodb: 27017
  redis: 6379
```

== MySQL + Redis

```yaml
# vm.yaml - Alternative to PostgreSQL
services:
  mysql:
    enabled: true
    database: myapp_dev
    user: root
    password: root
  redis:
    enabled: true

ports:
  mysql: 3306
  redis: 6379
```

:::

:::tip Default Credentials
All databases use simple defaults for development:
- PostgreSQL: `postgres/postgres`
- MySQL: `root/root`
- MongoDB: No authentication by default

Change these in production configurations!
:::

### Advanced Service Configuration

Additional service options for specialized use cases:

**Database services (PostgreSQL, MySQL, MongoDB):**
```yaml
services:
  postgresql:
    enabled: true
    version: "16"                # Specific version
    database: myapp_dev          # Database name
    user: postgres               # Username
    password: postgres           # Password
    memory_mb: 1024              # Memory limit (MB)
    seed_file: ./db/seed.sql     # SQL file to run on first start
    backup_on_destroy: true      # Auto-backup before vm destroy
```

**Docker-in-Docker:**
```yaml
services:
  docker:
    enabled: true
    buildx: true                 # Enable Docker Buildx for multi-platform builds
    driver: docker-container     # Buildx driver (docker-container, kubernetes)
```

**Headless Browser:**
```yaml
services:
  headless_browser:
    enabled: true
    display: ":99"               # X11 display number
    executable_path: /usr/bin/chromium  # Custom browser path
    share_microphone: false      # Share host microphone
```

**Service options:**
- **version**: Service version (postgresql: "16", "15", etc.)
- **memory_mb**: Memory limit in MB (prevents runaway processes)
- **seed_file**: SQL or script to run on first start (databases only)
- **backup_on_destroy**: Auto-backup before destroying VM (databases only)
- **buildx**: Enable Docker Buildx (docker service only)
- **driver**: Buildx driver type (docker service only)
- **display**: X11 display number (headless_browser only)
- **executable_path**: Custom binary path (headless_browser only)
- **share_microphone**: Enable microphone access (headless_browser only)

## Global Services Configuration

Global services are configured in `~/.vm/config.yaml` and serve **all** VMs on your system.

### Global Services

Shared across all VMs with automatic lifecycle management:

| Service | Purpose | Auto-managed |
|---------|---------|--------------|
| `docker_registry` | Docker image caching | Auto-start/stop |
| `auth_proxy` | Secure secret management | Auto-start/stop |
| `package_registry` | npm/pip/cargo package caching | Auto-start/stop |

**Lifecycle:**

Global services use reference counting:
1. **Auto-start** when first VM needs them
2. **Auto-stop** when last VM stops using them
3. **Zero maintenance** - fully automated lifecycle
4. **Shared resources** - all VMs benefit from the same service instance

### Docker Registry (Automatic Caching)

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
- **10-100x faster** Docker pulls after first cache
- **Bandwidth savings** - images pulled once, used many times
- **Zero maintenance** - automatic cleanup and management
- **Shared cache** - all VMs share the same image cache

### Auth Proxy (Secure Secrets)

Enable secure secret management across all VMs:

```yaml
# ~/.vm/config.yaml
services:
  auth_proxy:
    enabled: true
    port: 3090              # Port for auth proxy (default: 3090)
    token_expiry_hours: 24  # Token expiry (default: 24)
```

### Package Registry (Shared Cache)

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


## Operating System Configuration

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
  guest_os: linux              # Guest OS type: linux or macos
  rosetta: true                # Enable x86 emulation for Linux VMs
  disk_size: 60                # Disk: 60 (GB), "60gb", "50%"
  ssh_user: admin              # SSH username (default: admin)
  install_docker: false        # Install Docker in the VM
  storage_path: ~/.tart/vms    # Custom VM storage location
```

**Tart options:**
- **image**: OCI image to use (required)
- **guest_os**: `linux` or `macos` (auto-detected from image if omitted)
- **rosetta**: Enable x86 emulation on ARM (default: false)
- **disk_size**: VM disk size in GB (default: 50)
- **ssh_user**: SSH username for connecting (default: admin)
- **install_docker**: Install Docker in VM (default: false)
- **storage_path**: Custom VM storage directory (default: ~/.tart/vms)

**Requirements**: Apple Silicon Mac (M1/M2/M3/M4), Tart installed via `brew install cirruslabs/cli/tart`

### Box Configuration

The `vm.box` field specifies what to use as the base environment. It works across all providers with smart detection:

#### Simple String (Recommended)

For most cases, use a simple string:

```yaml
# Docker provider
vm:
  box: ubuntu:24.04          # Docker Hub image
  box: node:20-alpine        # Docker Hub image
  box: ./Dockerfile          # Build from local Dockerfile
  box: supercool.dockerfile  # Build from named Dockerfile

# Vagrant provider
vm:
  box: ubuntu/focal64        # Vagrant Cloud box
  box: hashicorp/bionic64    # Vagrant Cloud box

# Tart provider (macOS)
vm:
  box: ghcr.io/cirruslabs/ubuntu:latest       # OCI image
  box: ghcr.io/cirruslabs/macos-ventura:latest # macOS VM

# All providers
vm:
  box: "@my-snapshot"        # Restore from snapshot
```

#### Advanced Docker Build

For complex Docker builds with build arguments:

```yaml
vm:
  box:
    dockerfile: ./docker/dev.dockerfile
    context: .
    args:
      NODE_VERSION: "20"
      INSTALL_CHROMIUM: "true"
```

#### Detection Rules

The provider determines how to interpret the `box` string:

- **Starts with `@`** → Snapshot (all providers)
- **Starts with `./`, `../`, `/`** → Dockerfile path (Docker only)
- **Ends with `.dockerfile`** → Dockerfile (Docker only)
- **Contains `/` in Vagrant format** → Vagrant box
- **Registry format** → Docker/Tart image
- **Everything else** → Image name


## Language Runtimes

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
  npm: 10.2.4      # Specific npm version (usually managed by nvm)
  nvm: v0.40.3     # NVM version
  pnpm: latest     # pnpm version
```

**Note**: npm is typically installed automatically with Node.js via nvm. Only set `npm` explicitly if you need a specific version different from the one bundled with Node.

## Terminal Customization

### Available Themes

- `dracula` - Purple magic (default)
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

## Port Strategy

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

## Advanced Features

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
- **In-container helper** `vm-worktree` command for easy management (available inside VM via `vm ssh`)
- **Prompt indicator** shows current worktree
- Automatic detection and remounting of existing worktrees
- Support for multiple branches simultaneously

**Note**: The `vm-worktree` command is available inside the container, not as a `vm` CLI subcommand. Access it by running `vm ssh` first.

**Use Cases**:
- Developing multiple branches simultaneously
- Testing feature branches in isolation
- Quick branch switching without leaving container
- CI/CD workflows with parallel branch testing

**Example Workflows**:

**Creating worktrees from inside container:**
```bash
$ vm ssh
# Now inside container

$ vm-worktree add feature-x
# ✓ Worktree created: feature-x
# (worktree:feature-x) $  # Automatically navigated with prompt indicator

# Work on the feature
git commit -m "Add feature"

# On host (also works!)
cd ~/.vm/worktrees/myproject/feature-x
git status  # Shows your commits

# List all worktrees
vm-worktree list
# Worktrees:
#  feature-x
#  bugfix-123

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
host_sync:
  git_config: true  # default
```

To disable this feature, set `git_config` to `false`:

```yaml
# vm.yaml
host_sync:
  git_config: false
```

#### Timezone

Automatically detect and set the VM's timezone to match your host system.

```yaml
# vm.yaml
vm:
  timezone: auto  # default
```

To override the timezone, specify a valid IANA timezone name:

```yaml
# vm.yaml
vm:
  timezone: "America/New_York"
```

**Common timezones:**
- `America/New_York` - Eastern Time (US)
- `America/Chicago` - Central Time (US)
- `America/Denver` - Mountain Time (US)
- `America/Los_Angeles` - Pacific Time (US)
- `Europe/London` - UK
- `Europe/Paris` - Central European Time
- `Asia/Tokyo` - Japan Standard Time
- `Australia/Sydney` - Australian Eastern Time
- `UTC` - Coordinated Universal Time

**Full timezone list**: See the [IANA timezone database](https://en.wikipedia.org/wiki/List_of_tz_database_time_zones) for all 600+ valid timezone identifiers.

### Development Configuration

Enhanced developer workflows for SSH keys, dotfiles, and debugging support.

#### SSH Agent Forwarding

Securely use your host's SSH keys inside the VM without copying private keys:

```yaml
# vm.yaml
host_sync:
  ssh_agent: true      # Enable SSH agent forwarding
  ssh_config: true     # Mount ~/.ssh/config (optional, defaults to true)
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

#### Dotfiles Sync

Selectively sync your configuration files from host to VM for a consistent development environment:

```yaml
# vm.yaml
host_sync:
  dotfiles:
    - "~/.vimrc"            # Vim configuration
    - "~/.config/nvim"      # Neovim configuration directory
    - "~/.tmux.conf"        # Tmux configuration
    - "~/.gitconfig"        # Git configuration
    - "~/.bashrc"           # Bash configuration
    - "~/.zshrc"            # Zsh configuration
```

**What it does:**
- Mounts specified dotfiles from host into the VM (read-only)
- Supports both files and directories
- Expands `~` to your home directory automatically
- Preserves directory structure in container

**Path mapping:**
- `~/.vimrc` → `/home/developer/.vimrc` (in container)
- `~/.config/nvim` → `/home/developer/.config/nvim` (in container)
- Absolute paths (`/etc/foo`) stay the same

**Features:**
- **Read-only mounts** - Prevents accidental modification from inside VM
- **Selective sync** - Only mount what you need, keep containers minimal
- **Automatic validation** - Skips non-existent files with warnings
- **Path expansion** - Handles `~` expansion automatically

**Example configurations:**

**Vim/Neovim users:**
```yaml
host_sync:
  dotfiles:
    - "~/.vimrc"
    - "~/.config/nvim"
```

**Tmux + Zsh users:**
```yaml
host_sync:
  dotfiles:
    - "~/.tmux.conf"
    - "~/.zshrc"
    - "~/.oh-my-zsh"  # If using oh-my-zsh
```

**Full stack developers:**
```yaml
host_sync:
  dotfiles:
    - "~/.gitconfig"
    - "~/.npmrc"
    - "~/.pypirc"
    - "~/.cargo/config.toml"
```

**Notes:**
- Dotfiles are mounted read-only to prevent accidental changes
- Changes to dotfiles on host are immediately visible in VM
- Non-existent files are skipped with a warning during VM creation
- Use this for configuration files, not for workspace files

#### AI Tool Integration

Automatically sync AI coding assistant configurations into your VM for seamless AI-powered development.

**Enable all AI tools:**
```yaml
# vm.yaml
host_sync:
  ai_tools: true
```

**Granular control per tool:**
```yaml
# vm.yaml
host_sync:
  ai_tools:
    claude: true      # Claude Desktop (~/.config/claude)
    cursor: true      # Cursor editor (~/.cursor)
    aider: true       # Aider AI (~/.aider)
    codex: false      # OpenAI Codex (disabled)
    gemini: false     # Google Gemini (disabled)
```

**Supported AI tools:**
- **Claude** - Claude Desktop configuration and settings
- **Cursor** - Cursor editor AI configuration
- **Aider** - Aider AI coding assistant settings
- **Codex** - OpenAI Codex configuration
- **Gemini** - Google Gemini AI settings

**What gets synchronized:**
- API keys and authentication tokens
- Model preferences and settings
- Custom prompts and configurations
- Tool-specific preferences

**Security considerations:**
- AI tool configs often contain API keys - ensure your host machine is secure
- Configs are mounted read-only into the VM
- Changes made in VM don't affect host configurations
- Disable specific tools you don't use to minimize mounted secrets

**Use cases:**
- Consistent AI tooling across multiple projects
- Team standardization of AI tool configurations
- Quick onboarding with pre-configured AI assistants

#### Package Linking

Automatically detect and mount linked npm, pip, and cargo packages for monorepo and local package development.

**Enable all package managers:**
```yaml
# vm.yaml
host_sync:
  package_linking: true
```

**Selective package manager support:**
```yaml
# vm.yaml
host_sync:
  package_linking:
    npm: true      # Link npm packages (npm link, yarn link)
    pip: true      # Link Python packages (pip install -e)
    cargo: false   # Disable Rust package linking
```

**How it works:**
- Detects `npm link` / `yarn link` relationships automatically
- Finds `pip install -e` editable installations
- Mounts linked package source directories into VM
- Preserves package linking inside the VM
- Updates automatically when host links change

**npm/yarn example:**
```bash
# On host: Link shared component library
cd ~/projects/shared-components
npm link

cd ~/projects/my-app
npm link shared-components

# In VM: shared-components is automatically mounted and linked
vm ssh
npm list shared-components  # Shows linked local package
```

**pip example:**
```bash
# On host: Install local package in editable mode
cd ~/projects/my-library
pip install -e .

cd ~/projects/my-app
pip install -e ../my-library

# In VM: my-library source is automatically mounted
vm ssh
pip show my-library  # Shows editable install location
```

**Use cases:**
- Monorepo development with multiple packages
- Local package development and testing
- Shared internal libraries across projects
- Component library development

**Troubleshooting:**
- Ensure packages are linked before creating the VM
- Use `vm apply` to refresh mounts after changing links on host
- Check `vm status` to see which packages are mounted

### Dynamic Port Forwarding

#### Ephemeral Port Tunneling

Forward ports on-demand without permanent configuration. Perfect for debugging, testing, or temporary services.

```bash
# Forward localhost:8080 to container port 3000
vm tunnel create 8080:3000

# Forward to a specific container
vm tunnel create 8080:3000 myapp-dev

# List active port forwarding tunnels
vm tunnel list

# Stop a specific tunnel
vm tunnel stop 8080

# Stop all tunnels
vm tunnel stop --all
```

**What it does:**
- Creates ephemeral port forwarding using lightweight relay containers
- No permanent configuration needed - tunnels exist only while active
- Solves port conflicts between multiple VMs (each tunnel is independent)
- Automatic cleanup when containers stop

**Use Cases:**

**Debugging:**
```bash
# Start debugger in container
vm ssh
node --inspect=0.0.0.0:9229 app.js

# In another terminal, forward the debugger port
vm tunnel create 9229:9229

# Attach VS Code debugger to localhost:9229
# When done debugging:
vm tunnel stop 9229
```

**Testing multiple environments:**
```bash
# Each VM can use the same internal port
vm tunnel create 3000:3000 myapp-dev      # localhost:3000 → dev:3000
vm tunnel create 3001:3000 myapp-staging  # localhost:3001 → staging:3000
vm tunnel create 3002:3000 myapp-prod     # localhost:3002 → prod:3000

# Test all three simultaneously
curl localhost:3000  # hits dev
curl localhost:3001  # hits staging
curl localhost:3002  # hits prod
```

**Temporary services:**
```bash
# Forward a database for local inspection
vm tunnel create 5432:5432

# Use with local tools
psql -h localhost -p 5432 -U postgres

# Clean up when done
vm tunnel stop 5432
```

**How it works:**
- Creates a lightweight Alpine container with `socat`
- Shares network namespace with your VM container
- Routes traffic: `localhost:HOST → relay → container:GUEST`
- Tracked in `~/.config/vm/tunnels/active.json`

**Viewing active tunnels:**
```bash
vm tunnel list
```
```
🔀 Active Port Forwarding Tunnels

  localhost:9229 → myapp-dev:9229
    Relay: vm-port-forward-myapp-dev-9229 | Created: 2025-10-24T20:15:30Z

  localhost:3000 → myapp-staging:3000
    Relay: vm-port-forward-myapp-staging-3000 | Created: 2025-10-24T20:16:45Z
```

**Notes:**
- Tunnels are ephemeral - they don't survive system reboots
- Each tunnel uses a small relay container (~5MB memory)
- Conflicts with existing port bindings will error
- Use permanent ports in `vm.yaml` for production services
- Tunnels automatically stop when target container stops

**vs. Permanent Ports:**

| Feature | Dynamic (`vm tunnel create`) | Permanent (`ports:` in vm.yaml) |
|---------|---------------------------|--------------------------------|
| Configuration | Command-line, temporary | vm.yaml, permanent |
| Use case | Debugging, testing | Production services |
| Conflicts | Each VM independent | Must be unique across VMs |
| Lifecycle | Manual start/stop | Starts with VM |
| Overhead | Relay container per tunnel | Native Docker port mapping |

### VM Snapshots

#### Save and Restore Complete VM State

Snapshots capture your entire development environment including containers, volumes, and configurations.

```bash
# Create a snapshot
vm snapshot create my-working-state

# With description
vm snapshot create before-refactor --description "Stable state before major changes"

# List snapshots
vm snapshot list

# Restore from snapshot
vm snapshot restore my-working-state

# Delete old snapshots
vm snapshot delete old-experiment
```

**Storage Location:** `~/.config/vm/snapshots/<project>/<snapshot-name>/`

**What's Captured:**
- Container filesystems (installed packages, code changes)
- All volume data (databases, uploads, logs)
- Configuration files (vm.yaml, docker-compose.yml)
- Git commit hash and dirty state
- Creation timestamp and description

**Use Cases:**

**Safe Experimentation:**
```bash
vm snapshot create stable-baseline
# Try risky changes...
# Something broke? Restore:
vm snapshot restore stable-baseline
```

**Context Switching:**
```bash
# Save current feature work
vm snapshot create feature-x-wip

# Switch to urgent bugfix
git checkout hotfix-branch
vm destroy && vm create
# Work on hotfix...

# Back to feature work
vm snapshot restore feature-x-wip
```

**Team Collaboration:**
```bash
# Share working environment
vm snapshot create demo-ready
# Archive and send snapshot directory to teammate
# Teammate: vm snapshot restore demo-ready
```

**Configuration:**

```yaml
# ~/.vm/config.yaml
snapshots:
  path: "~/.config/vm/snapshots"  # Snapshot storage directory
```

**Notes:**
- Snapshots are project-specific and designed to be reused as base templates
- Large snapshots can consume significant disk space
- Use `vm snapshot delete <name>` to manually remove unwanted snapshots
- Quiesce flag (`--quiesce`) stops services for consistent snapshots

---

#### Migrating from Examples/Boxes

**Old Workflow (Deprecated):**
```bash
# Copy example configuration
cp examples/nextjs-app/vm.yaml ./
vm create
# Manually set up packages, databases, etc.
```

**Problems:**
- Lost all state on `vm destroy`
- Couldn't capture package installations
- Manual setup steps required after recreation
- Hard to share working environments

**New Workflow:**
```bash
# Initial setup (once)
vm init
vm create
# Install packages, configure databases, etc.

# Save working state
vm snapshot create my-configured-env

# Share with team or restore later
vm snapshot restore my-configured-env
```

**Benefits:**
- Complete state preservation
- One-command environment replication
- Safe experimentation with rollback
- Team collaboration ready

**If you previously used custom base images:**
- Create a VM with your desired packages installed
- Run `vm snapshot create base-environment`
- Share the snapshot instead of the Dockerfile
- Team members: `vm snapshot restore base-environment`

---

## Glossary

**VM (Virtual Machine / Development Environment)**
- Your isolated development environment
- Implemented as a Docker container (default), Vagrant virtual machine, or Tart VM depending on your provider
- Throughout documentation, "VM" refers to all provider types unless specifically noted

**Provider**
- The backend technology that runs your VM: Docker, Vagrant, or Tart
- Docker provides lightweight containers (fast, shared kernel)
- Vagrant provides full virtual machines (complete isolation, any OS)
- Tart provides macOS virtual machines on Apple Silicon

**Box / Image**
- The base template used to create your VM
- Docker: "image" (e.g., `ubuntu:24.04`)
- Vagrant: "box" (e.g., `ubuntu/jammy64`)
- Tart: "image" (e.g., `ghcr.io/cirruslabs/macos-ventura`)

**Host**
- Your physical computer where the VM Tool runs
- Your local filesystem, SSH keys, and configuration

**Guest**
- The VM environment running inside the host
- Where your project code executes

**Snapshot**
- A saved state of your entire VM including installed packages, configuration, and data
- Can be exported, shared, and restored on other machines

---

## Additional Resources

- [Examples Guide](../getting-started/examples.md) - Real-world configuration examples
- [Schema Reference](../../configs/schema/vm.schema.yaml) - Raw YAML schema with complete field definitions
- [Troubleshooting](troubleshooting.md) - Common configuration issues
