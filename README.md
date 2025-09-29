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

**Zero-Config Presets** — Language runtimes, databases, and development tools install automatically based on your project structure.

## 🚀 Quick Start

Get up and running in three commands:

```bash
# Install from Cargo (recommended)
cargo install vm

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

**From source:**
```bash
git clone <repository-url>
cd vm && ./install.sh
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

**Core Workflow** — The essential commands you'll use daily:

```bash
vm create                        # Create and configure VM
vm create --instance dev         # Create named instance
vm ssh [container]               # Enter VM (by name, ID, or project)
vm stop [container]              # Stop VM (preserves all data)
vm destroy [container]           # Delete VM completely
```

**Multi-Instance Management** — Handle multiple environments effortlessly:

```bash
vm list                          # List all VMs with status and resource usage
vm list --provider docker        # Filter by provider
vm destroy --all                 # Destroy all instances
vm destroy --pattern "*-dev"     # Pattern-based destruction
```

**Temporary Environments** — Perfect for quick experiments:

```bash
vm temp create ./src ./tests     # Instant VM with folder mounts
vm temp ssh                      # Enter temp VM
vm temp destroy                  # Clean up when done
```

**Daily Operations** — Monitor and control your environments:

```bash
vm status [container]            # Show VM status and health
vm logs [container]              # View VM logs
vm exec [container] "cmd"        # Execute commands inside VM
vm restart [container]           # Restart VM
vm provision [container]         # Re-run provisioning
```

## 🧪 Temporary VMs

**Instant Development Environments** — Spin up isolated environments in seconds for testing ideas or debugging:

```bash
vm temp create ./src ./tests ./docs:ro
vm temp ssh              # Enter and start coding immediately
vm temp destroy          # Clean up when finished
```

**Dynamic Mount Management** — Add and remove folders without recreating the VM:

```bash
vm temp mount ./new-feature     # Add directories while working
vm temp unmount ./old-code      # Remove specific mount
vm temp unmount --all           # Remove all mounts
vm temp mounts                  # List current mounts
```

## 🔐 Auth Proxy

**Centralized secrets management** — Store and manage credentials securely across VMs:

```bash
# Auth proxy for secrets management
vm auth status                   # Service status
vm auth add openai sk-xxx        # Store API key
vm auth list                     # List stored secrets
vm auth remove <name>            # Remove a secret
vm auth interactive              # Interactively add secrets
```

## 📦 Package Registry

**Private package registry for npm, pip, and cargo** — Host your own packages with automatic upstream fallback:

```bash
# Package management
vm pkg status                    # Server status and package counts
vm pkg add                       # Auto-detect and publish from current directory
vm pkg add --type python         # Specific package type
vm pkg list                      # List all packages
vm pkg remove                    # Interactive removal
vm pkg use --shell bash          # Generate shell configuration for package managers
```

**Installation:**
```bash
./install.sh                    # Install VM CLI with package management
```

**Features:**
- **Multi-registry support** — PyPI, npm, and Cargo in one server
- **Upstream fallback** — Serves local packages first, fetches from official registries when needed
- **Zero dependencies** — Single binary, no database required
- **Integrated workflow** — Package management built into VM tool

## ⚙️ Configuration

**Most projects need zero configuration**, but when you do need customization, it's straightforward:

```yaml
# Minimal configuration
os: ubuntu
project:
  name: my-project

# Port mapping for web development
ports:
  frontend: 3000
  backend: 3001

# Resource allocation
provider: docker
vm:
  memory: 4096
  cpus: 2

# Database services
services:
  postgresql:
    enabled: true
    version: "15"
```

**Preset Management** — Apply pre-configured stacks instantly:

```bash
vm config preset django          # Apply Django preset to config
vm config preset --list          # List available presets
vm config preset --show nodejs   # Show specific preset details
```

## 🔧 Debugging & Support

**When things go wrong** — Get detailed information and quick fixes:

```bash
LOG_LEVEL=DEBUG vm create    # Detailed output for troubleshooting
vm logs                      # View service logs
```

**Need help?**
- **Issues**: Report problems at the project repository
- **Quick fixes**: Run `vm destroy && vm create` to resolve most issues
- **Stuck containers**: Use `vm list` to see all instances, then `vm destroy <name>` to clean up

---

## 📖 Complete Command Reference

**VM Lifecycle** — Core commands for managing virtual machines:

```bash
vm create [--instance name] [--force]  # Create and provision a new VM
vm start [container]                   # Start a VM
vm stop [container]                    # Stop a VM or force-kill specific container
vm restart [container]                 # Restart a VM
vm provision [container]               # Re-run VM provisioning
vm destroy [container] [--force] [--all] [--pattern "*"]  # Destroy VMs
vm status [container]                  # Show VM status and health
vm ssh [container] [--path dir]        # Connect to VM via SSH
```

**Temporary VMs** — Ephemeral environments for quick testing:

```bash
vm temp create <folders>     # Create temporary VM with mounts
vm temp ssh                  # Connect to temporary VM via SSH
vm temp status               # Show temporary VM status
vm temp destroy              # Destroy temporary VM
vm temp mount <path>         # Add mount to running temporary VM
vm temp unmount <path>       # Remove mount from temporary VM
vm temp unmount --all        # Remove all mounts
vm temp mounts               # List current mounts
vm temp list                 # List all temporary VMs
vm temp stop                 # Stop temporary VM
vm temp start                # Start temporary VM
vm temp restart              # Restart temporary VM
```

**Configuration Management** — Customize and control VM settings:

```bash
vm init                      # Initialize a new VM configuration file
vm validate                  # Validate VM configuration
vm config get [field]        # Get configuration values
vm config set <field> <value> # Set configuration value
vm config unset <field>      # Remove configuration field
vm config preset <names>     # Apply configuration presets
vm config preset list        # List available presets
vm config preset --show <name> # Show preset details
vm config ports --fix        # Manage port configuration and resolve conflicts
```

**Advanced Operations** — Power user commands for complex workflows:

```bash
vm list [--provider name] [--verbose]    # List all VMs with status and resource usage
vm exec [container] <command>            # Execute commands inside VM
vm logs [container]                      # View VM logs
vm --config custom.yaml ssh [container] # Use specific config
```

**System Management** — Update and maintain the VM tool itself:

```bash
vm update [--version v1.2.3]    # Update to latest or specific version
vm uninstall [--keep-config]    # Uninstall vm from the system
vm doctor                        # Run comprehensive health checks
```