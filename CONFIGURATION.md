# ⚙️ Configuration Guide

Complete reference for configuring your VM development environment with YAML.

## 📖 Table of Contents

- [Quick Start](#-quick-start)
- [Configuration Files](#-configuration-files)
- [Full Reference](#-full-reference)
- [Services](#-services)
- [Language Runtimes](#-language-runtimes)
- [Terminal Customization](#-terminal-customization)
- [Migration from JSON](#-migration-from-json)
- [Examples](#-examples)

## 🚀 Quick Start

### Minimal Setup

Most projects just need ports. Everything else has smart defaults:

```yaml
ports:
  frontend: 3020
  backend: 3022
```

### Add PostgreSQL

Want PostgreSQL? Just add:

```yaml
ports:
  frontend: 3020
  backend: 3022
  postgresql: 3025
services:
  postgresql:
    enabled: true
```

## 📁 Configuration Files

### Automatic Discovery

The `vm` command automatically searches for `vm.yaml` configuration:

1. **Current directory**: `./vm.yaml`
2. **Parent directory**: `../vm.yaml`
3. **Grandparent directory**: `../../vm.yaml`
4. **Defaults**: If no config found, uses built-in defaults

This means you can run `vm create` from anywhere in your project tree!

### IDE Support

For autocompletion and validation in your editor:

```yaml
# yaml-language-server: $schema=./vm.schema.yaml
ports:
  frontend: 3020
```

## 📋 Full Reference

```yaml
version: "1.0"  # Configuration format version

provider: docker  # or "vagrant" - defaults to "docker"

project:
  name: my-app  # VM/container name & prompt
  hostname: dev.my-app.local  # VM/container hostname
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

## 🔄 Migration from JSON

If you have existing `vm.json` configuration files, easily migrate to YAML:

### Migration Commands

```bash
# Check if migration is needed
vm migrate --check

# Preview the migration (dry run)
vm migrate --dry-run

# Perform the migration
vm migrate

# Migration options
vm migrate --input old-config.json --output new-config.yaml
vm migrate --backup              # Create backup (default: enabled)
vm migrate --no-backup           # Skip backup creation
vm migrate --force               # Skip confirmation prompts
```

### What Migration Does

1. **Converts JSON to YAML**: Transforms your `vm.json` into readable `vm.yaml`
2. **Adds Version Field**: Automatically injects `version: "1.0"` for compatibility
3. **Creates Backup**: Saves original as `vm.json.bak` (unless --no-backup)
4. **Validates Result**: Ensures migrated configuration is valid
5. **Preserves Settings**: All services, ports, aliases, and customizations maintained

### Example Migration

**Before (vm.json):**
```json
{
  "project": {
    "name": "my-app"
  },
  "services": {
    "postgresql": {
      "enabled": true
    }
  }
}
```

**After (vm.yaml):**
```yaml
version: "1.0"
project:
  name: my-app
services:
  postgresql:
    enabled: true
```

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

## 📝 Examples

### Minimal Frontend Project

```yaml
project:
  name: my-frontend
ports:
  dev: 3000
  storybook: 6006
npm_packages:
  - "@storybook/cli"
  - prettier
```

### Full-Stack Application

```yaml
project:
  name: fullstack-app
ports:
  frontend: 3000
  backend: 3001
  postgresql: 5432
  redis: 6379
services:
  postgresql:
    enabled: true
    database: app_dev
  redis:
    enabled: true
npm_packages:
  - nodemon
  - "@types/node"
aliases:
  dev: "pnpm dev"
  api: "cd backend && pnpm start"
```

### Rust + Python Project

```yaml
project:
  name: polyglot-project
cargo_packages:
  - cargo-watch
  - serde_json
pip_packages:
  - requests
  - pytest
  - black
ports:
  rust_server: 8080
  python_api: 8000
aliases:
  rust-dev: "cd rust-service && cargo watch -x run"
  py-test: "cd python-service && pytest"
```

### Mobile Development

```yaml
project:
  name: mobile-backend
vm:
  memory: 6144  # More RAM for mobile tooling
services:
  postgresql:
    enabled: true
  redis:
    enabled: true
  docker:
    enabled: true  # For containerized services
ports:
  api: 3000
  postgresql: 5432
  redis: 6379
  websocket: 3001
npm_packages:
  - "@react-native-community/cli"
  - expo-cli
```