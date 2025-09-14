# 📋 Configuration Schema Reference

Complete reference for the YAML configuration format, including all available fields, types, and validation rules.

## 🏗️ Schema Structure

```yaml
# Schema version for compatibility
version: "1.0"

# Simple OS-based configuration (recommended)
os: string  # ubuntu|macos|debian|alpine|linux|auto

# Advanced provider configuration
provider: string  # docker|vagrant|tart

# Project settings
project:
  name: string                    # Required: VM/container identifier
  hostname: string               # Required: VM hostname
  workspace_path: string         # Path inside VM (default: /workspace)
  env_template_path: string      # Environment template file
  backup_pattern: string         # Database backup file pattern
  persist_databases: boolean     # Enable database persistence

# VM/Container resources
vm:
  box: string                    # Vagrant box name
  memory: integer               # RAM in MB (default: 4096)
  cpus: integer                 # CPU cores (default: 2)
  user: string                  # VM user (default: developer)
  port_binding: string          # IP binding (127.0.0.1 or 0.0.0.0)
  timeout: integer              # Creation timeout in seconds

# Language runtime versions
versions:
  node: string                  # Node.js version (default: 22.11.0)
  nvm: string                   # NVM version (default: v0.40.3)
  pnpm: string                  # pnpm version (default: latest)

# Port mappings
ports:
  [name]: integer              # Named port mappings

# Service configurations
services:
  postgresql:
    enabled: boolean
    database: string
    user: string
    password: string
  redis:
    enabled: boolean
  mongodb:
    enabled: boolean
  docker:
    enabled: boolean
  headless_browser:
    enabled: boolean

# Package installations
npm_packages: string[]          # Global npm packages
cargo_packages: string[]        # Global cargo packages (installs Rust)
pip_packages: string[]          # Global pip packages (installs Python)

# Shell customization
aliases:
  [name]: string               # Custom shell aliases

# Environment variables
environment:
  [name]: string               # Environment variables

# Terminal appearance
terminal:
  emoji: string                # Prompt emoji (default: ⚡)
  username: string             # Prompt username
  theme: string                # Color theme
  show_git_branch: boolean     # Show git branch in prompt
  show_timestamp: boolean      # Show timestamp in prompt

# Tart-specific settings (Apple Silicon)
tart:
  image: string                # VM image URL
  rosetta: boolean             # Enable Rosetta 2 for x86 emulation
  disk_size: integer           # Disk size in GB
  ssh_user: string             # SSH username
```

## 🎯 Field Details

### OS Configuration
```yaml
os: ubuntu  # Recommended approach
```

**Valid Values**:
- `ubuntu` - Docker/Vagrant, 4GB RAM, full development stack
- `macos` - Tart provider, 8GB RAM, native Apple Silicon
- `debian` - Docker/Vagrant, 2GB RAM, lightweight setup
- `alpine` - Docker only, 1GB RAM, minimal footprint
- `linux` - Docker/Vagrant, 4GB RAM, generic Linux
- `auto` - Auto-detect from project files

**Auto-Selection Logic**:
- Apple Silicon Mac + `os: macos` → Tart provider
- Any platform + `os: ubuntu` → Docker provider (or Vagrant if available)
- Docker preferred over Vagrant when both available

### Provider Configuration
```yaml
provider: docker  # Explicit provider selection
```

**Valid Values**:
- `docker` - Lightweight containers, fast startup
- `vagrant` - Full VMs, maximum isolation
- `tart` - Apple Silicon native virtualization

**Provider Availability**:
- Docker: Requires Docker/Docker Desktop
- Vagrant: Requires Vagrant + VirtualBox/VMware
- Tart: Requires macOS on Apple Silicon + Tart CLI

### Project Settings
```yaml
project:
  name: my-app                     # Required
  hostname: dev.my-app.local       # Required
  workspace_path: /workspace       # Optional, default shown
  env_template_path: .env.template # Optional
  backup_pattern: "*backup*.sql.gz" # Optional
  persist_databases: true          # Optional, default: false
```

**Field Validation**:
- `name`: Must be valid container/VM name (alphanumeric, hyphens, underscores)
- `hostname`: Must be valid hostname format
- `workspace_path`: Absolute path inside VM
- `backup_pattern`: Shell glob pattern for SQL backup files

### VM Resources
```yaml
vm:
  memory: 4096      # MB, minimum 1024
  cpus: 2          # Cores, minimum 1
  user: developer   # VM username
  port_binding: 127.0.0.1  # or "0.0.0.0"
```

**Resource Defaults by OS**:
```yaml
ubuntu: { memory: 4096, cpus: 2 }
macos:  { memory: 8192, cpus: 4 }  # Tart optimized
debian: { memory: 2048, cpus: 2 }
alpine: { memory: 1024, cpus: 1 }
```

### Port Configuration
```yaml
ports:
  frontend: 3000
  backend: 3001
  database: 5432
  cache: 6379
```

**Validation Rules**:
- Port numbers: 1024-65535 (avoid system ports)
- No duplicate port numbers
- Automatic conflict detection with running services

**Common Port Patterns**:
```yaml
# Web development
ports:
  dev: 3000
  api: 3001

# Full-stack with services
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
  mongodb:
    enabled: false
  docker:
    enabled: true        # Docker-in-Docker
  headless_browser:
    enabled: false       # Chrome/Chromium for testing
```

**Service Defaults**:
- All services disabled by default
- PostgreSQL: `database: postgres`, `user: postgres`, `password: postgres`
- Services run inside the VM/container

### Package Management
```yaml
npm_packages:
  - prettier
  - eslint
  - "@types/node"

cargo_packages:        # Triggers Rust installation
  - cargo-watch
  - tokei

pip_packages:          # Triggers Python installation
  - black
  - pytest
  - requests
```

**Installation Behavior**:
- `npm_packages`: Installed globally with npm
- `cargo_packages`: Installs Rust toolchain + packages
- `pip_packages`: Installs Python via pyenv + packages

### Terminal Customization
```yaml
terminal:
  emoji: "🚀"
  username: developer
  theme: dracula
  show_git_branch: true
  show_timestamp: false
```

**Available Themes**:
- `dracula` (default), `gruvbox_dark`, `solarized_dark`, `nord`
- `monokai`, `one_dark`, `catppuccin_mocha`, `tokyo_night`

## 🔍 Schema Validation

### Required Fields
```yaml
# Minimal valid configuration
project:
  name: required-string
  hostname: required-hostname
```

### Validation Rules
1. **Project name**: Alphanumeric with hyphens/underscores only
2. **Hostname**: Valid DNS hostname format
3. **Ports**: Integer range 1024-65535, no duplicates
4. **Memory**: Minimum 1024MB
5. **CPUs**: Minimum 1 core
6. **OS**: Must be valid OS identifier
7. **Provider**: Must be available on system

### Configuration Validation
```bash
# Validate configuration file
vm validate

# Validate specific file
vm validate --config custom.yaml

# Verbose validation output
vm validate --verbose
```

## 📝 Example Schemas

### Minimal Configuration
```yaml
# Absolute minimum - relies on auto-detection
project:
  name: my-project
  hostname: dev.my-project.local
```

### OS-Based Configuration
```yaml
# Recommended approach
os: ubuntu
project:
  name: my-project
  hostname: dev.my-project.local
ports:
  web: 3000
```

### Explicit Provider Configuration
```yaml
# Maximum control
provider: docker
project:
  name: my-project
  hostname: dev.my-project.local
vm:
  memory: 6144
  cpus: 3
  port_binding: "0.0.0.0"
services:
  postgresql:
    enabled: true
    database: myapp_dev
ports:
  frontend: 3000
  backend: 3001
  postgresql: 5432
```

### Framework-Specific Schema
```yaml
# React development
os: ubuntu
project:
  name: react-app
  hostname: dev.react-app.local
ports:
  dev: 3000
  storybook: 6006
npm_packages:
  - "@storybook/cli"
  - prettier
terminal:
  emoji: "⚛️"
  theme: one_dark
```

## 🔄 Schema Evolution

### Version Compatibility
```yaml
version: "1.0"  # Current schema version
```

**Migration Handling**:
- Forward compatibility maintained
- Deprecated fields show warnings
- Schema version used for validation

### Deprecation Timeline
- **v1.0**: Current stable schema
- **v2.0**: Planned provider interface changes
- **v3.0**: Planned configuration restructuring

## 🛡️ Security Considerations

### Sensitive Data
```yaml
# ❌ Never commit sensitive data
environment:
  API_KEY: "secret-key"  # Use env templates instead

# ✅ Use environment templates
project:
  env_template_path: .env.template
```

### Network Security
```yaml
# Local development (default)
vm:
  port_binding: 127.0.0.1

# Network accessible (use cautiously)
vm:
  port_binding: "0.0.0.0"
```

### Service Security
```yaml
# Development passwords (not for production)
services:
  postgresql:
    enabled: true
    password: postgres  # Change for production use
```

## 🔧 Advanced Schema Features

### Conditional Configuration
```yaml
# Different configs based on environment
# Use separate files: dev.yaml, prod.yaml, test.yaml
vm --config dev.yaml create
```

### Configuration Inheritance
```yaml
# Base configuration can be extended
# Presets provide base configurations
# User config overrides preset values
# CLI flags override config values
```

### Dynamic Values
```yaml
# Some values support dynamic resolution
project:
  name: ${PROJECT_NAME:-default-name}  # Environment variable fallback
```