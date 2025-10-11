# 🚀 Quick Start Guide

Setup guide for getting your development environment running.

## 🎯 Minimal Setup

```bash
# 1. Clone the repository and build from source
git clone https://github.com/goobits/vm.git
cd vm
./install.sh --build-from-source

# 2. Create environment (auto-detects your project)
vm create

# 3. Enter your development environment
vm ssh
```

The tool automatically:
- Detects your project type (React, Django, Rails, Vue, etc.)
- Configures appropriate services and tools
- Sets up port forwarding and file sync
- Installs language runtimes and dependencies

## 🛠️ Common Workflows

### Web Development
```bash
# React/Vue/Angular projects
cd my-frontend-app
vm create                    # → Node.js, npm, dev server ready
vm ssh
npm run dev                  # Runs on auto-configured ports
```

### API Development
```bash
# Django/Flask/Rails projects
cd my-api-project
vm create                    # → Python/Ruby + PostgreSQL + Redis
vm ssh
python manage.py runserver   # Database already configured
```

### Quick Experiments
```bash
# Test code in isolated environment
vm temp create ./src ./tests # Mount specific folders
vm temp ssh                  # Jump in and experiment
vm temp destroy              # Clean up when done
```

## ⚙️ Basic Customization

Only customize if the auto-detection doesn't work for you:

```yaml
# vm.yaml - minimal override
os: ubuntu
provider: docker
project:
  name: my-project
ports:
  frontend: 3000
  backend: 8000
```

## 🎮 Essential Commands

```bash
# Daily workflow
vm create        # Create and configure VM
vm ssh           # Enter the VM
vm stop          # Stop VM (keeps data)
vm start         # Resume stopped VM
vm destroy       # Delete completely

# Quick info
vm status        # Check if running
vm list          # Show all VMs
vm logs          # View service logs
```

## 🧪 Temporary VMs

Suitable for testing, code reviews, or experiments:

```bash
vm temp create ./feature-branch  # Mount specific directories
vm temp ssh                      # Enter temp environment
vm temp destroy                  # Clean up when done
```

## 🆘 Need Help?

- **Not working?** Try `vm destroy && vm create` to reset
- **Missing features?** Check the [Presets Guide](../user-guide/presets.md) for available configurations
- **Custom setup?** See the [Configuration Guide](../user-guide/configuration.md)
- **All commands?** View the [CLI Reference](../user-guide/cli-reference.md)

## Next Steps

- [📖 Configuration Guide](../user-guide/configuration.md) - Customize your environment
- [🎯 Presets Guide](../user-guide/presets.md) - Understand auto-detection
- [🛠️ CLI Reference](../user-guide/cli-reference.md) - Complete command list