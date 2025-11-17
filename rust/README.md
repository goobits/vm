# VM Workspace Management System

A comprehensive workspace management system for creating and managing development environments.

## Overview

This project provides tools for managing containerized development workspaces with features like:
- Multiple provider support (Docker, etc.)
- Template-based workspace creation
- Package management and registry
- Authentication and access control
- API service with web UI

## Components

- `vm` - Core CLI tool for local workspace management
- `vm-api` - REST API service for hosted workspace management (Phase 1)
- `vm-orchestrator` - Backend orchestration and database layer
- `vm-provider` - Abstraction layer for different VM providers
- `vm-config` - Configuration management
- `vm-package-server` - Package registry server
- `vm-package-manager` - Package installation and management

## Getting Started

### Prerequisites

- Rust 1.70+
- Docker (for containerized workspaces)
- SQLite 3

### Building

```bash
# Build all components
cargo build --release

# Build specific components
cargo build -p vm
cargo build -p vm-api
```

### Running the API Service (Phase 1)

The vm-api service provides a REST API and web UI for managing hosted workspaces:

```bash
# Start the API server
cd /workspace/rust
cargo run --bin vm-api

# Access the web UI
open http://localhost:3000/app

# Or use the API directly
curl -H "x-user: testuser" http://localhost:3000/api/v1/workspaces
```

The service runs on port 3000 by default. See [vm-api/README.md](vm-api/README.md) for configuration options.

**Phase 1 Features:**
- Create/list/delete workspaces via API
- Web UI at `/app` with auto-refresh
- Background provisioning (creates actual Docker containers)
- TTL-based automatic cleanup
- Mock authentication (Phase 2 will add real GitHub OAuth)

### Running the CLI

```bash
# Install the CLI
cargo install --path vm

# Create a workspace
vm create my-workspace --template nodejs

# List workspaces
vm list

# Connect to a workspace
vm ssh my-workspace
```

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run integration tests (requires Docker)
cargo test --features integration -- --ignored
```

### Code Quality

```bash
# Run clippy
cargo clippy --all-targets --all-features

# Format code
cargo fmt
```

## Documentation

- [Architecture](ARCHITECTURE.md) - System architecture and design
- [API Reference](docs/user-guide/api.md) - REST API documentation
- [Development Guide](CLAUDE.md) - Notes for AI-assisted development

## License

See individual component licenses for details.
