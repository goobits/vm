# Presets

Skip manual configuration - VM auto-detects your project type and applies optimized defaults. When you run `vm up` without a config file, the tool analyzes your project files and selects the appropriate preset.

## Available Presets

### vibe (Recommended)
Full-featured development environment with precompiled `@vibe-box` snapshot
- All common languages: Node.js, Python, Ruby, Rust, Go
- AI coding tools: Claude Code, Gemini CLI
- Docker, Kubernetes tools
- Database clients
- Standard development utilities
- Host sync for git config and AI tools

```bash
vm config preset vibe
```

### base
Minimal development environment
- Basic shell utilities and git
- Standard ports configuration
- Minimal resource allocation

### tart-linux
Linux VMs on Apple Silicon (Tart provider)
- Optimized for ARM64
- Linux-specific configurations

### tart-macos
macOS VMs on Apple Silicon (Tart provider)
- Native macOS virtualization
- macOS-specific tools

### tart-ubuntu
Ubuntu VMs on Apple Silicon (Tart provider)
- Ubuntu-optimized settings
- ARM64 Ubuntu configuration

**To list all available presets**:
```bash
vm config preset --list
```

## Usage

### Automatic Detection
```bash
# Analyzes project files and applies appropriate preset
vm up
```

### Apply Specific Preset
```bash
vm config preset vibe
vm config preset base
```

## How It Works

1. **Application**: Merges preset configuration with your `vm.yaml`
   - Your settings take precedence
   - Presets fill in missing values
   - Services and tools are additive

2. **Validation**: Ensures compatibility and resolves conflicts

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

### Using the Plugin System (Recommended)

Create a custom preset plugin:

```bash
# Create plugin template
vm plugin new my-preset --type preset

# This creates:
# ~/.vm/plugins/presets/my-preset/
#  ├── plugin.yaml    # Plugin metadata
#  ├── preset.yaml    # Preset configuration
#  └── README.md      # Documentation
```

Edit the generated files:

**plugin.yaml** (metadata):
```yaml
name: my-preset
version: 1.0.0
description: Custom development environment
author: Your Name
plugin_type: preset
```

**preset.yaml** (configuration):
```yaml
npm_packages:
  - your-package

pip_packages:
  - your-python-package

services:
  - postgresql
  - redis

environment:
  CUSTOM_VAR: value
```

Then use it:
```bash
vm config preset my-preset
vm up
```

### File-based presets

You can add presets in `configs/presets/`:
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

**Note:** Plugin-based presets are preferred as they provide better organization, versioning, and validation.

For more details, see the [Plugin Guide](./plugins.md).
