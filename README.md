# 🚀 Goobits VM

Create development environments that automatically configure themselves based on your project type. Zero configuration required for most projects.

## 📚 Documentation

- **[CLAUDE.md](CLAUDE.md)** - Development notes and testing guide

## ✨ Key Features

**Smart Detection** — Automatically recognizes Next.js, React, Angular, Vue, Django, Flask, Rails, Node.js, Python, Rust, Go, PHP, Docker, and Kubernetes projects without manual configuration.

**Multi-Instance Workflows** — Run multiple environments per project. Create `myproject-dev`, `myproject-staging`, and `myproject-prod` instances that operate independently.

**Universal Container Management** — Reference containers by partial IDs, full names, or project names. All commands work consistently across Docker, Vagrant, and Tart providers.

**Secure by Default** — Docker containers ship with hardened security settings and isolated networking. No exposure of sensitive data between environments.

**Lightning Fast** — Most Docker environments spin up in under 60 seconds. Temporary VMs launch instantly with only the folders you need.

**Intelligent File Sync** — Edit code locally with your favorite tools while execution happens in the VM. Changes appear immediately without manual copying.

**Git Worktree Support** — Automatic detection and proper volume mounting for Git worktrees, enabling multi-branch development workflows.

**Zero-Config Presets** — Language runtimes, databases, and development tools install automatically based on your project structure.

## Prerequisites

Before you begin, make sure you have the following tools installed.

### 1. Rust Toolchain

The VM CLI is built with Rust, so you'll need the Rust toolchain (including `cargo`, the Rust package manager). If you don't have it, the official `rustup` installer is the best way to get started.

- **Installation:** [https://rustup.rs](https://rustup.rs)
- **Why?** `cargo` is used to install the `vm` binary from its source package.

### 2. Docker

VM uses Docker as its default "provider" to create lightweight, isolated development environments. Make sure the Docker daemon is running before using `vm` commands.

- **Installation:** [https://docs.docker.com/get-docker/](https://docs.docker.com/get-docker/)
- **Why?** Docker manages the lifecycle of your development containers.

---

## 🚀 Quick Start

**Note**: Pre-compiled binaries are not yet available. The recommended installation method is to build from source.

Get up and running in three commands:

```bash
# Clone the repository and run the install script
git clone https://github.com/goobits/vm.git
cd vm
./install.sh --build-from-source

# Create your environment — it detects your project automatically
vm create

# Jump into your development environment
vm ssh
```

**Multiple environments per project:**
```bash
vm create --instance dev     # Creates myproject-dev
vm create --instance prod    # Creates myproject-prod
vm ssh myproject-dev         # Connect to specific instance
```

```yaml
# Optional: vm.yaml for custom configuration
os: ubuntu
provider: docker  # or vagrant for full isolation
project:
  name: my-project
```

## 🛠️ Environment Types

**Docker (Default)** — Lightweight containers using 1-4GB RAM. Perfect for most development workflows with fast startup times.

**Vagrant (Maximum Isolation)** — Full virtual machines for security-critical projects or when you need complete OS isolation.

**Smart Project Detection** — The system analyzes your codebase and automatically configures the right tools:

```bash
cd my-react-app && vm create     # → Node.js + npm + dev tools
cd my-django-api && vm create    # → Python + PostgreSQL + Redis
cd fullstack-app && vm create    # → Multiple presets combined
```

Choose your provider explicitly:
```yaml
# vm.yaml for Vagrant isolation
provider: vagrant
project:
  name: my-project
```

## 🎮 Commands

### Core Workflow
The essential commands you'll use daily:
```bash
vm create              # Create and configure a new environment
vm start               # Start an existing environment
vm stop                # Stop an environment (preserves all data)
vm destroy             # Delete an environment completely
vm ssh                 # Jump into your environment
vm exec "npm install"  # Execute a command inside your environment
```

### Environment Management
Commands for managing and monitoring your environments:
```bash
vm list                # List all environments with their status
vm status              # Show the status and health of an environment
vm logs                # View the logs for an environment
vm provision           # Re-run the provisioning process
vm restart             # Restart an environment
```

### Configuration (`vm config`)
Manage your `vm.yaml` configuration from the command line:
```bash
vm init                # Create a new vm.yaml configuration file
vm config validate     # Validate the current configuration
vm config show         # Show the loaded configuration
vm config set <k> <v>  # Set a configuration value
vm config preset django  # Apply the Django preset to your config
```

### Temporary Environments (`vm temp`)
Spin up isolated environments in seconds for testing or debugging:
```bash
vm temp create ./src   # Create a temporary environment with ./src mounted
vm temp ssh            # SSH into the temporary environment
vm temp destroy        # Clean up the temporary environment
vm temp list           # List all active temporary environments
```

### Secrets Management (`vm auth`)
Store and manage credentials securely across all your environments:
```bash
vm auth add openai sk-xxx  # Store an API key
vm auth list               # List all stored secrets
vm auth remove openai      # Remove a secret
```

### Package Registry (`vm pkg`)
Host your own private packages for `npm`, `pip`, and `cargo`:
```bash
vm pkg add             # Publish a package from the current directory
vm pkg list            # List all packages in the registry
vm pkg remove my-pkg   # Remove a package from the registry
```

---
## 📖 Complete Command Reference

### Core Commands
```bash
vm create                # Create and provision a new VM
vm start                 # Start a stopped VM
vm stop                  # Stop a running VM
vm restart               # Restart a VM
vm provision             # Re-run VM provisioning
vm destroy               # Destroy a VM
vm status                # Show VM status and health
vm ssh                   # Connect to a VM via SSH
vm exec <command>        # Execute a command inside a VM
vm logs                  # View VM logs
vm list                  # List all VMs
```

### Configuration (`vm config`)
```bash
vm init                  # Initialize a new vm.yaml configuration file
vm config validate       # Validate the VM configuration
vm config show           # Show the loaded configuration
vm config get [field]    # Get a configuration value
vm config set <f> <v>    # Set a configuration value
vm config unset <field>  # Remove a configuration field
vm config preset <name>  # Apply a configuration preset
vm config ports --fix    # Manage port configuration
```

### Temporary VMs (`vm temp`)
```bash
vm temp create <folders> # Create a temporary VM with mounted folders
vm temp ssh              # Connect to the temporary VM
vm temp status           # Show temporary VM status
vm temp destroy          # Destroy the temporary VM
vm temp mount <path>     # Add a mount to a running temporary VM
vm temp unmount <path>   # Remove a mount from a temporary VM
vm temp mounts           # List current mounts
vm temp list             # List all temporary VMs
```

### Plugins (`vm plugin`)
```bash
vm plugin list           # List installed plugins
vm plugin info <name>    # Show plugin details
vm plugin install <path> # Install a plugin from a directory
vm plugin remove <name>  # Remove an installed plugin
vm plugin new <name>     # Create a new plugin template
vm plugin validate <name> # Validate a plugin's configuration
```

### Secrets Management (`vm auth`)
```bash
vm auth add <name> <value> # Store a secret
vm auth list               # List stored secrets
vm auth remove <name>      # Remove a secret
```

### Package Registry (`vm pkg`)
```bash
vm pkg add [--type <t>]  # Publish a package from the current directory
vm pkg list              # List all packages in the registry
vm pkg remove <name>     # Remove a package from the registry
```

### System Management
```bash
vm doctor                # Run comprehensive health checks
vm update                # Update to the latest or a specific version
vm uninstall             # Uninstall vm from the system
```

---

## 🏗️ Architecture & Development

### Rust Crate Overview

The VM tool is built from multiple focused Rust crates:

| Crate | Purpose | Key Responsibilities |
|-------|---------|---------------------|
| `vm` | Main CLI application | Command orchestration, user interaction |
| `vm-core` | Foundation utilities | Error handling, file system, command execution |
| `vm-messages` | Message templates | Centralized user-facing text and messages |
| `vm-cli` | CLI formatting | Structured output, message building |
| `vm-config` | Configuration management | Config parsing, validation, project detection |
| `vm-provider` | Provider abstraction | Docker/Vagrant/Tart lifecycle management |
| `vm-temp` | Temporary VMs | Ephemeral environment management |
| `vm-platform` | Cross-platform support | OS detection, platform-specific paths |
| `vm-package-manager` | Package integration | npm/pip/cargo link detection |
| `vm-package-server` | Private registry | Local package hosting for npm/pip/cargo |
| `vm-auth-proxy` | Authentication | Centralized secrets management |
| `vm-docker-registry` | Docker registry | Local Docker image hosting |
| `vm-installer` | Installation | Self-installation and updates |
| `version-sync` | Build tool | Cross-workspace version synchronization |

**For detailed architecture documentation**, see:
- [rust/ARCHITECTURE.md](rust/ARCHITECTURE.md) - Comprehensive crate architecture
- [docs/development/architecture.md](docs/development/architecture.md) - High-level overview
- [CLAUDE.md](CLAUDE.md) - Development and testing guide