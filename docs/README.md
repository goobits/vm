# VM Documentation

Documentation for the VM development environment tool.

User and contributor guides live under `docs/`. Package-level API and
architecture notes stay beside the package they describe.

## Canonical Owners

- [Quick Start](getting-started/quick-start.md) owns the first-use workflow.
- `vm --help` is the installed-version source of truth. The
  [CLI Reference](user-guide/cli-reference.md) is the canonical documented
  public command inventory and owns target-selection behavior.
- [Configuration Guide](user-guide/configuration.md) owns `vm.yaml` semantics.
- [Package Infrastructure](user-guide/package-infrastructure.md) owns shared
  package registries, managed source releases, automatic consumer upgrades,
  and recovery workflows.
- [Troubleshooting](user-guide/troubleshooting.md) owns recovery guidance.
- [Testing Guide](development/testing.md) owns supported quality commands.
- [Architecture](development/architecture.md) owns code boundaries.

Other guides should link to these owners instead of copying their inventories.

---

## New User? Start Here

**Quick Start** (5 min)
[getting-started/quick-start.md](getting-started/quick-start.md)
Get your first VM running.

**Installation Guide** (15 min)
[getting-started/installation.md](getting-started/installation.md)
Platform-specific installation, prerequisites, and troubleshooting.

**Configuration Examples**
[getting-started/examples.md](getting-started/examples.md)
Configurations for common project types.

---

## Using VM

### Core Guides

**Configuration Guide**
[user-guide/configuration.md](user-guide/configuration.md)
Reference for vm.yaml and global configuration.

**CLI Reference**
[user-guide/cli-reference.md](user-guide/cli-reference.md)
Complete public command inventory and targeting rules.

**Troubleshooting**
[user-guide/troubleshooting.md](user-guide/troubleshooting.md)
Common issues and fixes by symptom.

### Advanced Features

**Presets**
[user-guide/presets.md](user-guide/presets.md)
Using built-in presets for fast project setup.

**Plugins**
[user-guide/plugins.md](user-guide/plugins.md)
Extending VM functionality with custom plugins.

**Shared Services**
[user-guide/shared-services.md](user-guide/shared-services.md)
Global services shared across all VMs.

**Package Infrastructure**
[user-guide/package-infrastructure.md](user-guide/package-infrastructure.md)
Private npm, Cargo, and Python package workflows shared by Docker and Tart.

---

## Contributing

**Development Guide**
[development/guide.md](development/guide.md)
Set up your development environment and run tests.

**Testing Guide**
[development/testing.md](development/testing.md)
Supported test commands, isolation rules, and troubleshooting.

**Architecture**
[development/architecture.md](development/architecture.md)
System design, component interactions, and codebase structure.

**Publishing**
[development/publishing.md](development/publishing.md)
Release process and package publishing.

**Contributing Guidelines**
[../CONTRIBUTING.md](../CONTRIBUTING.md)
How to contribute code, docs, and report issues.

---

## Documentation Conventions

**Command Examples**
Commands use `$` to indicate user prompt. Output shown only when relevant for understanding.

**YAML Examples**
Use `# ...` to indicate continuing content. Comments highlight non-obvious configuration choices.

**File Paths**
All paths are relative to your project root unless noted otherwise.

**Platform Notes**
Platform-specific instructions marked with **macOS**, **Linux**, or **Windows** labels.
