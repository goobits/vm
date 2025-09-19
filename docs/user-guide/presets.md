# 📦 VM Tool Presets

## Overview
Preset system that auto-configures VMs based on detected project type.

## Available Presets

### base
Core development environment with essential tools
- Basic shell utilities and git
- Standard ports configuration
- Minimal resource allocation

### django
Python/Django web development
- Python 3.x runtime
- PostgreSQL & Redis services
- Django development tools
- Ports: 8000, 5432, 6379

### docker
Container-based development
- Docker & docker-compose
- Container orchestration tools
- Kubernetes support (optional)

### kubernetes
Kubernetes development and testing
- kubectl, helm, minikube
- Container runtime
- Kubernetes dashboard

### next
Next.js applications
- Node.js & npm packages
- Next.js optimizations
- React development tools
- Ports: 3000, 3001

### nodejs
Node.js/JavaScript development
- Node.js & npm/pnpm
- Common build tools
- Ports: 3000, 3001, 8080

### python
Python development environment
- Python 3.x with pip
- Virtual environment tools
- Common Python packages

### rails
Ruby on Rails development
- Ruby runtime with rbenv
- Rails framework
- PostgreSQL & Redis
- Ports: 3000, 5432, 6379

### react
React frontend development
- Node.js & npm packages
- Vite, webpack dev server
- React testing tools
- Ports: 3000, 3001, 5173

### rust
Rust development environment
- Rust toolchain & cargo
- Development tools
- Common crates

### tart-linux
Linux VMs on Apple Silicon (Tart)
- Optimized for ARM64
- Linux-specific configurations

### tart-macos
macOS VMs on Apple Silicon (Tart)
- Native macOS virtualization
- macOS-specific tools

### tart-ubuntu
Ubuntu VMs on Apple Silicon (Tart)
- Ubuntu-optimized settings
- ARM64 Ubuntu configuration

## Usage

### Automatic Detection
```bash
# Analyzes project files and applies appropriate preset
vm create
```

### Apply Specific Preset
```bash
vm config preset django
vm config preset react
```

## How It Works

1. **Detection**: Scans for framework-specific files:
   - `package.json` → nodejs/react/next
   - `requirements.txt` → python
   - `Gemfile` → rails
   - `manage.py` → django
   - `Dockerfile` → docker
   - `Cargo.toml` → rust
   - Kubernetes manifests → kubernetes

2. **Application**: Merges preset configuration with your `vm.yaml`
   - Your settings take precedence
   - Presets fill in missing values
   - Services and tools are additive

3. **Validation**: Ensures compatibility and resolves conflicts

## Customization

Override preset values in `vm.yaml`:
```yaml
# Your settings override preset defaults
os: ubuntu
ports:
  frontend: 3010  # Overrides preset's 3000
services:
  postgresql:
    enabled: false  # Disable preset service
```

## Creating Custom Presets

Add new presets in `configs/presets/`:
```yaml
# configs/presets/custom.yaml
preset:
  name: "Custom Stack"
  description: "My custom development setup"

npm_packages:
  - your-package

services:
  your-service:
    enabled: true

ports:
  - 9000
```

For preset creation details, check the existing presets in the `configs/presets/` directory for examples.