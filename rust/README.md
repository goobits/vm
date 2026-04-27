# VM Workspace Management System

A comprehensive workspace management system for creating and managing development environments.

## Overview

This project provides tools for managing containerized development workspaces with features like:
- Multiple provider support (Docker, Tart)
- Template-based workspace creation
- Package management and registry
- Authentication and access control

## Components

- `vm` - Core CLI tool for workspace management
- `vm-provider` - Abstraction layer for different VM providers (Docker, Tart, Podman)
- `vm-config` - Configuration management
- `vm-package-server` - Package registry server
- `vm-package-manager` - Package installation and management

## Getting Started

### Prerequisites

- Rust 1.70+
- Docker (for containerized workspaces)

### Building

```bash
# Build all components
cargo build --release

# Build specific components
cargo build -p vm
```

### Running the CLI

```bash
# Install the CLI
cargo install --path vm

# Create and start a workspace
vm start

# List workspaces
vm status

# Connect to a workspace
vm ssh
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
- [Development Guide](CLAUDE.md) - Notes for AI-assisted development

## License

See individual component licenses for details.
