# ⚙️ Configuration Guide

Complete reference for configuring your VM development environment with YAML.

## 📖 Table of Contents

- [Quick Start](#-quick-start)
- [Configuration Files](#-configuration-files)
- [Full Reference](#-full-reference)
- [Services](#-services)
- [Language Runtimes](#-language-runtimes)
- [Terminal Customization](#-terminal-customization)
- [Advanced Features](#-advanced-features)

For configuration examples, see the [Examples Guide](../getting-started/examples.md).

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
  memory: 8192   # 8GB RAM
  cpus: 4        # 4 CPU cores
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
  memory: 4096  # RAM in MB
  cpus: 2  # CPU cores
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

All services run **inside** the VM/container and are accessed via `localhost`.

### PostgreSQL

```yaml
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
services:
  redis:
    enabled: true
ports:
  redis: 6379  # Access via localhost:6379
```

### MongoDB

```yaml
services:
  mongodb:
    enabled: true
ports:
  mongodb: 27017  # Access via localhost:27017
```

### Docker-in-Docker

```yaml
services:
  docker:
    enabled: true  # Allows running docker commands inside VM
```

### Headless Browser (for testing)

```yaml
services:
  headless_browser:
    enabled: true  # Installs Chrome/Chromium for testing
```

### Docker Registry (Automatic Caching) 🆕

Enable intelligent Docker image caching that works like a browser cache - completely invisible while dramatically speeding up Docker pulls:

```yaml
docker_registry: true  # That's it! Zero-configuration caching
```

**What it does:**
- **Auto-starts** when your VM needs it
- **Caches all Docker images** locally for instant pulls
- **Self-manages** with automatic cleanup of old images
- **Auto-configures** Docker daemon to use local mirror
- **Stops automatically** when no VMs need it

**Advanced configuration** (optional):
```yaml
docker_registry: true
docker_registry_config:
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
  disk_size: 60  # GB
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

### Database Persistence

```yaml
project:
  persist_databases: true  # Store data in .vm/data/
```
- Survives VM rebuilds
- Add `.vm/` to `.gitignore`

### Environment Templates

```yaml
project:
  env_template_path: "backend/.env.template"
```
- Automatically copies template to `.env` in VM

### Database Backups

Drop `.sql.gz` files matching your pattern in the project:

```yaml
project:
  backup_pattern: "*backup*.sql.gz"
```

They'll auto-restore on provision!

## 📚 Additional Resources

- **[Examples Guide](../getting-started/examples.md)** - Real-world configuration examples
- **[Schema Reference](../api/configuration-schema.md)** - Complete field documentation
- **[Troubleshooting](troubleshooting.md)** - Common configuration issues