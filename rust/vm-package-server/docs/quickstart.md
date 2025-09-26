# 🚀 Quick Start Guide

Get your package server running in 30 seconds!

## Installation

```bash
# Install globally (recommended)
./install.sh

# Or build from source
cargo build --release
```

## Starting the Server

### Option 1: Direct Run (Simplest)
```bash
# Start server and auto-configure package managers
pkg-server start

# Custom port
pkg-server start --port 9000
```

### Option 2: Docker (One Flag!)
```bash
# Add --docker flag for containerized deployment
pkg-server start --docker

# That's it! It automatically:
# ✅ Builds the image if needed
# ✅ Starts the container
# ✅ Mounts your data directory
# ✅ Configures your package managers
```

## Client Setup

Once your server is running, configure any machine to use it:

### Automatic Setup (Recommended)
```bash
# On ANY machine that needs to use the package server:
curl http://YOUR_SERVER_IP:3080/setup.sh | bash

# All package managers are now configured!
```

### What Gets Configured
- **pip** → `~/.pip/pip.conf` points to your server
- **npm** → Registry set to your server
- **cargo** → `~/.cargo/config.toml` uses your server

### Manual Setup (If Needed)

**Python/pip:**
```bash
pip install --index-url http://localhost:3080/pypi/simple/ package-name
```

**Node.js/npm:**
```bash
npm config set registry http://localhost:3080/npm/
npm install package-name
```

**Rust/Cargo:**
```toml
# Add to ~/.cargo/config.toml
[registries.local]
index = "sparse+http://localhost:3080/cargo/"
```

## Publishing Packages

### Auto-detect and Publish
```bash
# From your project directory
pkg-server add

# Or specify types
pkg-server add --type python,npm
```

### Manual Publishing

**Python:**
```bash
twine upload --repository-url http://localhost:3080/pypi/ dist/*
```

**npm:**
```bash
npm publish --registry http://localhost:3080/npm/
```

**Cargo:**
```bash
cargo publish --registry local
```

## Docker Integration

### Docker Compose Example
```yaml
services:
  package-server:
    image: goobits-pkg-server:latest
    ports:
      - "3080:3080"
    volumes:
      - ./data:/home/appuser/data

  my-app:
    build: .
    depends_on:
      - package-server
    command: |
      sh -c "curl http://package-server:3080/setup.sh | bash && python app.py"
```

### CI/CD Pipeline
```yaml
before_script:
  - curl http://package-server:3080/setup.sh | bash

script:
  - pip install -r requirements.txt  # Uses cache!
  - npm install  # Uses cache!
```

## Testing Your Setup

```bash
# Test pip
pip install requests
# Should show: "Looking in indexes: http://YOUR_SERVER:3080/pypi/simple/"

# Test npm
npm install express
# Should show your server in the registry URL

# Test cargo
cargo add serde --registry local
```

## Common Commands

```bash
pkg-server start          # Start server
pkg-server stop           # Stop and restore original settings
pkg-server add            # Publish packages
pkg-server list           # List all packages
pkg-server remove         # Remove packages interactively
pkg-server status         # Show server status
```

## Rollback Client Configuration

To restore original package manager settings:

```bash
# Remove configurations
rm ~/.pip/pip.conf
npm config delete registry
rm ~/.cargo/config.toml

# Or use the stop command
pkg-server stop
```

## Benefits

- **⚡ Speed**: Packages cached locally after first download
- **💾 Bandwidth**: Download once, use everywhere
- **🔒 Reliability**: Works even if PyPI/npm/crates.io are down
- **🎯 Simplicity**: One command setup
- **🐳 Docker-friendly**: Built-in container support

That's it! Your private package server is ready. 🎉