# 🚀 Goobits VM

![Build Status](https://img.shields.io/github/actions/workflow/status/goobits/vm/ci.yml?branch=main&style=for-the-badge)
![License](https://img.shields.io/github/license/goobits/vm?style=for-the-badge)
![Crates.io](https://img.shields.io/crates/v/vm-cli?style=for-the-badge)
![Codecov](https://img.shields.io/codecov/c/github/goobits/vm?style=for-the-badge)

Create development environments that automatically configure themselves based on your project type. Zero configuration required for most projects.

## 📚 Documentation

- **[Development Guide](docs/DEVELOPMENT.md)** - Development notes and testing guide
- **[Testing Guide](docs/TESTING.md)** - Comprehensive testing documentation

## ✨ Key Features

**Smart Detection** — Automatically recognizes Next.js, React, Angular, Vue, Django, Flask, Rails, Node.js, Python, Rust, Go, PHP, Docker, and Kubernetes projects without manual configuration.

**Multi-Instance Workflows** — Run multiple environments per project. Create `myproject-dev`, `myproject-staging`, and `myproject-prod` instances that operate independently.

**Universal Container Management** — Reference containers by partial IDs, full names, or project names. All commands work consistently across Docker, Vagrant, and Tart providers.

**Secure by Default** — Docker containers ship with hardened security settings and isolated networking. No exposure of sensitive data between environments.

**Lightning Fast** — Most Docker environments spin up in under 60 seconds. Temporary VMs launch instantly with only the folders you need.

**Intelligent File Sync** — Edit code locally with your favorite tools while execution happens in the VM. Changes appear immediately without manual copying.

**Git Worktree Support** — Create worktrees from inside containers that work on host too! Use `vm-worktree add feature-x` for instant branch switching without leaving your container.

**Zero-Config Presets** — Language runtimes, databases, and development tools install automatically based on your project structure.

**Shared Database Services** — Optional, shared instances of PostgreSQL, Redis, and MongoDB that run on the host and are accessible to all VMs. This saves memory, speeds up startup, and persists data across VM rebuilds. See the [Shared Services User Guide](docs/user-guide/shared-services.md) for details.

## Prerequisites

### Required
- **Docker** (with proper permissions)
  - Linux: Add user to docker group
    ```bash
    sudo usermod -aG docker $USER
    newgrp docker
    # Note: You may need to log out/in for changes to take effect
    ```
  - macOS: Install Docker Desktop
  - Windows: Install Docker Desktop with WSL2
- **Rust toolchain** (1.70 or later)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Git**

### Verify Installation
```bash
docker --version
docker ps  # Should not error
cargo --version
```

### Troubleshooting Docker

**"Permission Denied" Error on Linux**

If you see an error like `permission denied while trying to connect to the Docker daemon socket`, it means your user account is not in the `docker` group.

**Solution:**
1.  Add your user to the `docker` group:
    ```bash
    sudo usermod -aG docker $USER
    ```
2.  Apply the new group membership. You can either log out and log back in, or run the following command to start a new shell session with the correct permissions:
    ```bash
    newgrp docker
    ```

**Docker Daemon Not Running**

If you get an error like `Cannot connect to the Docker daemon. Is the docker daemon running on this host?`, it means the Docker service isn't active.

**Solution:**
- **Linux (systemd):**
  ```bash
  sudo systemctl start docker
  ```
- **macOS/Windows:** Make sure Docker Desktop is running.

---

## 🚀 Quick Start

**Note**: Pre-compiled binaries are not yet available. The recommended installation method is to build from source.

Get up and running in a few simple steps:

**1. Install `vm` from Source**
```bash
# Clone the repository and run the install script
git clone https://github.com/goobits/vm.git
cd vm
./install.sh --build-from-source
```

**2. Initialize Your Project**
Navigate to your project's directory and run `vm init` to generate a `vm.yaml` file. The tool will auto-detect your project type and suggest a configuration.
```bash
cd /path/to/your-project
vm init
```

**3. Create and Connect to Your Environment**
Once you're ready, create the environment and SSH into it.
```bash
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
vm ssh                 # Jump into your environment (auto-detects new worktrees)
vm exec "npm install"  # Execute a command inside your environment
```

### Environment Management
Commands for managing and monitoring your environments:
```bash
vm list                # List all environments with their status
vm status              # Show the status and health of an environment
vm logs                # View the logs for an environment
vm apply               # Re-apply configuration to running environment
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
vm apply                 # Re-apply configuration to running VM
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

### Database Management (`vm db`)
```bash
vm db list               # List all managed databases
vm db backup <db_name>   # Create a backup of a database
vm db restore <db_name>  # Restore a database from the latest backup
vm db size <db_name>     # Show the size of a database
vm db export <db_name>   # Export a database to a file
vm db import <db_name>   # Import a database from a file
vm db reset <db_name>    # Reset a database to its initial state
```

### System Management
```bash
vm doctor                # Run comprehensive health checks
vm update                # Update to the latest or a specific version
vm uninstall             # Uninstall vm from the system
```

---

## 🏗️ Architecture & Development

### Testing

The project uses a staged testing strategy to ensure both speed and thoroughness.

- **Unit Tests**: Fast, in-memory tests that run in seconds. Use `make test-unit`.
- **Integration Tests**: Slower, more comprehensive tests that may require Docker. Use `make test-integration`.
- **Full Suite**: Run all tests with `make test`.

For a detailed guide on running, debugging, and writing tests, please see the **[Development Guide](docs/DEVELOPMENT.md)** and **[Testing Guide](docs/TESTING.md)**.

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
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) - Development and testing guide