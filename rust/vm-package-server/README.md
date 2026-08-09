# 📦 Goobits Package Server
Private package registry server with upstream fallback for PyPI, npm, and Cargo packages.

## ✨ Key Features
- **🌐 Multi-registry** - PyPI, npm, and Cargo in a single server
- **🔄 Upstream fallback** - Serves local packages first, fetches from official registries when needed
- **🎯 Zero dependencies** - Single static binary, no database required
- **🐳 Container ready** - Multi-stage Docker build with non-root user
- **🔒 Path protected** - Validates all file operations against traversal attacks
- **💾 File storage** - Simple directory structure, easy backup and migration

## 🚀 Quick Start

```bash
# Install and start
./install.sh                    # Install to system PATH
pkg-server start                # Start server and configure package managers

# For containerized deployment, see the Docker Deployment section below.
```

For detailed instructions, see [docs/quickstart.md](docs/quickstart.md)

## 📚 CLI Commands

See [docs/cli-reference.md](docs/cli-reference.md) for complete command documentation.

**Quick Reference:**
```bash
pkg-server start [--port 3080]    # Start server
pkg-server add                               # Publish packages
pkg-server list                              # List packages
pkg-server status                            # Server status
```

## ⚙️ Client Configuration

### 🐍 Python / pip

```bash
# Quick setup (server auto-configures on start)
pip install package-name         # Automatically uses local server

# Manual configuration
pip install --index-url http://localhost:3080/pypi/simple/ package-name

# Publishing packages
twine upload --repository-url http://localhost:3080/pypi/ dist/*

# Permanent configuration in ~/.pypirc
[local]
repository = http://localhost:3080/pypi/
```

### 📦 Node.js / npm

```bash
# Quick setup (server auto-configures on start)
npm install package-name         # Automatically uses local server

# Manual configuration
npm config set registry http://localhost:3080/npm/
npm install package-name

# Publishing packages
npm publish --registry http://localhost:3080/npm/

# Per-project configuration in .npmrc
registry=http://localhost:3080/npm/
```

### 🦀 Rust / Cargo

```toml
# Add to ~/.cargo/config.toml
[registries.local]
index = "sparse+http://localhost:3080/cargo/"
```

```bash
# Installing packages
cargo add package-name --registry local

# Publishing packages
cargo publish --registry local
```

## 🛠️ Advanced Configuration

```bash
# Server options
pkg-server start \
  --host 127.0.0.1 \            # Bind address (default: 127.0.0.1)
  --port 3080 \                 # Port number (default: 3080)
  --data /var/packages          # Storage directory (default: ./data)

# Environment variables
RUST_LOG=debug pkg-server start  # Enable debug logging
RUST_LOG=trace pkg-server start  # Maximum verbosity
```

## 📁 Storage Structure

```
data/
├── pypi/
│   └── packages/              # Python wheels and tarballs
│       ├── package-1.0.0.whl
│       └── package-1.0.0.whl.meta
├── npm/
│   ├── metadata/              # Package metadata JSON
│   │   └── package-name.json
│   └── tarballs/              # Package tarballs
│       └── package-1.0.0.tgz
└── cargo/
    ├── index/                 # Registry index files
    │   └── he/ll/hello-world
    └── crates/                # Crate files
        └── hello-world-0.1.0.crate
```

## 🌐 API Endpoints

### PyPI
```bash
GET  /pypi/simple/                     # Package index
GET  /pypi/simple/{package}/           # Package versions
GET  /pypi/packages/{filename}         # Download package
POST /pypi/                            # Upload package
```

### npm
```bash
GET  /npm/{package}                    # Package metadata
GET  /npm/{package}/-/{tarball}        # Download package
PUT  /npm/{package}                    # Publish package
```

### Cargo
```bash
GET  /cargo/config.json                # Registry config
GET  /cargo/{path}/{crate}             # Crate index
GET  /cargo/api/v1/crates/{name}/{version}/download  # Download crate
PUT  /cargo/api/v1/crates/new          # Publish crate
```

### Generic Registry API (NEW)
```bash
GET    /api/{registry}/packages/count        # Count packages by registry
GET    /api/{registry}/packages              # List packages by registry
DELETE /api/{registry}/packages/{name}/{version}  # Delete package version
DELETE /api/{registry}/packages/{name}       # Delete all versions
```

Where `{registry}` can be: `pypi`, `npm`, or `cargo`

Examples:
```bash
curl http://localhost:3080/api/pypi/packages/count
curl http://localhost:3080/api/npm/packages
curl -X DELETE http://localhost:3080/api/pypi/packages/mypackage/1.0.0
```

### Web UI
```bash
GET  /                                  # Dashboard with package counts
GET  /ui/{type}                         # Browse packages by type
GET  /setup.sh                          # Client setup script
```

## 🧪 Development

```bash
# Build from source
cargo build --release

# Install locally for development
./install.sh

# Run tests
cargo test

# Development mode with hot reload
cargo watch -x run

# Code quality
cargo fmt                      # Format code
cargo clippy -- -D warnings    # Lint with warnings as errors

# Release build with optimizations
cargo build --release

# Using development binary directly (without installing)
pkg-server --help
```

## 🐳 Docker Deployment

```bash
# Build image
docker build -t goobits-pkg-server:latest -f docker/server/Dockerfile .

# Run with persistent storage
docker run -d \
  --name pkg-server \
  --restart unless-stopped \
  -p 3080:3080 \
  -e PKG_SERVER_AUTH_TOKEN="replace-with-a-strong-token" \
  -v $(pwd)/data:/home/appuser/data \
  goobits-pkg-server:latest

# View logs
docker logs -f pkg-server

# Stop server
docker stop pkg-server && docker rm pkg-server
```

## 📖 Documentation

- **[Quick Start](docs/quickstart.md)** - Installation and setup guide
- **[Configuration](docs/configuration.md)** - Server configuration and authentication
- **[API Reference](docs/api-reference.md)** - Complete API endpoint documentation
- **[CLI Reference](docs/cli-reference.md)** - Complete command-line documentation
- **[Contributing](docs/contributing.md)** - Development guidelines
- **[Changelog](CHANGELOG.md)** - Version history

## 💡 Support

- **Issues**: [GitHub Issues](https://github.com/goobits/goobits-pkg-server/issues)
- **Discussions**: [GitHub Discussions](https://github.com/goobits/goobits-pkg-server/discussions)
- **Security**: Report vulnerabilities to security@goobits.io

## 📝 License

MIT License
