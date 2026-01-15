# Plugins

Extend VM with custom presets and services that integrate seamlessly with the core tool. Share configurations across teams, add support for specialized frameworks, or bundle company-specific tooling - plugins make VM configurations portable and reusable.

## Quick Start

### Using Existing Plugins

```bash
# List all installed plugins
vm plugin list

# View plugin details
vm plugin info nodejs

# Use a plugin preset
vm config preset nodejs
vm create
```

### Creating a New Plugin

```bash
# Create a preset plugin
vm plugin new my-preset --type preset

# Create a service plugin
vm plugin new my-service --type service
```

This generates a complete plugin structure at `~/.vm/plugins/presets/my-preset/` or `~/.vm/plugins/services/my-service/`.

---

## Plugin Types

### Preset Plugins

Preset plugins define development environment configurations:
- Package installations (apt, npm, pip, cargo)
- Environment variables
- Service dependencies
- Port configurations

**Use cases:**
- Framework-specific environments (Django, Rails, React)
- Language toolchains (Python, Rust, Go)
- Development stacks (LAMP, MEAN, JAMstack)

### Service Plugins

Service plugins define background services that can run in VMs:
- Database servers (PostgreSQL, MySQL, MongoDB)
- Cache systems (Redis, Memcached)
- Message queues (RabbitMQ, Kafka)
- Custom services

**Use cases:**
- Third-party services
- Custom microservices
- Development dependencies

---

## Plugin Structure

### Directory Layout

```
~/.vm/plugins/
├── presets/
│   ├── nodejs/
│   │   ├── plugin.yaml    # Plugin metadata
│   │   ├── preset.yaml    # Preset configuration
│   │   └── README.md      # Documentation
│   └── my-custom/
│       ├── plugin.yaml
│       ├── preset.yaml
│       └── README.md
└── services/
    └── my-service/
        ├── plugin.yaml
        ├── service.yaml
        └── README.md
```

---

## Creating Preset Plugins

### Step 1: Generate Template

```bash
vm plugin new awesome-stack --type preset
```

### Step 2: Edit plugin.yaml

```yaml
name: awesome-stack
version: 1.0.0
description: Full-stack JavaScript development with modern tools
author: Your Name
plugin_type: preset
```

**Required fields:**
- `name` - Unique plugin identifier (lowercase, hyphens)
- `version` - Semantic version (e.g., 1.0.0)
- `description` - Brief plugin description
- `author` - Plugin author name
- `plugin_type` - Must be "preset" for preset plugins

**Optional fields:**
- `homepage` - Plugin website or repository URL
- `license` - License identifier (e.g., MIT, Apache-2.0)
- `tags` - Search tags (e.g., ["javascript", "fullstack"])

### Step 3: Edit preset.yaml

```yaml
# Package installations
npm_packages:
  - typescript
  - eslint
  - prettier
  - vite

pip_packages:
  - black
  - ruff

cargo_packages:
  - ripgrep

# Services to enable
services:
  - postgresql
  - redis

# Environment variables
environment:
  NODE_ENV: development
  DATABASE_URL: postgresql://localhost/myapp
  REDIS_URL: redis://localhost:6379

# Shell aliases
aliases:
  dev: npm run dev
  test: npm test
```

**Available fields:**
- `npm_packages` - Node.js packages to install globally
- `pip_packages` - Python packages to install
- `cargo_packages` - Rust packages to install
- `services` - Services to enable (must exist in service registry)
- `environment` - Environment variables (key: value)
- `aliases` - Shell aliases (key: command)

### Step 4: Document Your Plugin

Edit `README.md` with:
- Plugin purpose and use cases
- Prerequisites and dependencies
- Configuration examples
- Troubleshooting tips

### Step 5: Test Your Plugin

```bash
# Validate plugin structure
vm plugin validate awesome-stack

# Test in a new project
cd /tmp/test-project
vm config preset awesome-stack
vm create
vm ssh
```

---

## Creating Service Plugins

### Step 1: Generate Template

```bash
vm plugin new elasticsearch --type service
```

### Step 2: Edit plugin.yaml

```yaml
name: elasticsearch
version: 1.0.0
description: Elasticsearch search and analytics engine
author: Your Name
plugin_type: service
```

### Step 3: Edit service.yaml

```yaml
service:
  name: elasticsearch
  display_name: Elasticsearch
  description: Search and analytics engine
  port: 9200
  health_endpoint: http://localhost:9200/_cluster/health
  supports_graceful_shutdown: true
```

**Required fields:**
- `name` - Service identifier (used in vm.yaml)
- `display_name` - Human-readable name
- `description` - Service description
- `port` - Primary service port
- `health_endpoint` - URL to check service health
- `supports_graceful_shutdown` - Whether service handles SIGTERM gracefully

---

## Installing Plugins

### From Directory

```bash
# Install from local directory
vm plugin install ./my-plugin

# Install from absolute path
vm plugin install /home/user/plugins/custom-preset
```

### From Git Repository (Future)

```bash
# Not yet implemented
vm plugin install https://github.com/user/vm-plugin-name
```

---

## Managing Plugins

### List Plugins

```bash
vm plugin list
```

Output:
```
Installed plugins:

Presets:
  nodejs (v1.0.0)
    Node.js Development - Optimized for JavaScript/TypeScript projects
    Author: VM Tool Team

  django (v1.0.0)
    Django Development - Python web framework with PostgreSQL and Redis
    Author: VM Tool Team
```

### View Plugin Details

```bash
vm plugin info nodejs
```

Output:
```
Plugin: nodejs

  Version: 1.0.0
  Type: preset
  Author: VM Tool Team
  License: MIT
  Description: Node.js Development - Optimized for JavaScript/TypeScript projects
  Minimum VM Version: >=0.1.0

  Location: /home/user/.vm/plugins/presets/nodejs
```

### Remove Plugin

```bash
vm plugin remove nodejs
```

### Validate Plugin

```bash
# Validate specific plugin
vm plugin validate my-preset

# Validation checks:
# - Required files exist (plugin.yaml, content file)
# - YAML syntax is valid
# - Required fields are present
# - Version follows semver
# - Port numbers are valid (if applicable)
# - Service references exist
```

---

## Plugin Discovery

VM Tool automatically discovers plugins in:
1. `~/.vm/plugins/presets/` - Preset plugins
2. `~/.vm/plugins/services/` - Service plugins

**Discovery order:**
1. Plugins (user-installed, highest priority)
2. Embedded presets (system presets like `base`, `tart-*`)
3. File-based presets (`configs/presets/`)

This means plugins override embedded presets with the same name.

---

## Best Practices

### Naming Conventions

- Use lowercase with hyphens: `my-awesome-preset`
- Be descriptive: `react-typescript` not `rt`
- Avoid generic names: `nodejs-backend` not `backend`

### Versioning

Follow [Semantic Versioning](https://semver.org/):
- `1.0.0` - Major.Minor.Patch
- Increment MAJOR for breaking changes
- Increment MINOR for new features
- Increment PATCH for bug fixes

### Documentation

Include in README.md:
- **What it does** - Clear description
- **When to use it** - Use cases
- **How to use it** - Step-by-step guide
- **Examples** - Real-world usage
- **Troubleshooting** - Common issues

### Testing

Before publishing:
1. Validate syntax: `vm plugin validate <name>`
2. Test installation: `vm plugin install .`
3. Test preset application: `vm config preset <name>`
4. Test VM creation: `vm create`
5. Verify packages installed: `vm ssh` then check

---

## Plugin Validation

### Automatic Validation

Plugins are validated on:
- Installation (`vm plugin install`)
- Discovery (`vm plugin list`)
- Application (`vm config preset`)

### Validation Rules

**Plugin metadata:**
- `name` must be lowercase alphanumeric with hyphens
- `version` must follow semver (e.g., 1.0.0)
- `description` is required
- `plugin_type` must be "preset" or "service"

**Preset content:**
- Service references must exist in service registry
- Port numbers must be 1-65535
- Package names should be non-empty

**Service content:**
- Port must be valid (1-65535)
- Health endpoint must be valid URL
- Name must match plugin name

### Manual Validation

```bash
vm plugin validate my-plugin
```

Output:
```
✓ Plugin structure valid
✓ Required files present
✓ YAML syntax valid
✓ Metadata complete
✓ Port numbers valid
✓ Service references valid

Plugin 'my-plugin' is valid!
```

---

## Troubleshooting

### Plugin Not Found

**Problem:** `vm plugin list` doesn't show your plugin

**Solutions:**
1. Check location: `ls ~/.vm/plugins/presets/` or `ls ~/.vm/plugins/services/`
2. Verify structure: Must have `plugin.yaml` and content file
3. Check naming: Plugin directory name should match `name` in plugin.yaml
4. Validate: `vm plugin validate <name>`

### Validation Errors

**Problem:** Plugin validation fails

**Common issues:**
1. **Invalid YAML syntax** - Use YAML linter to check
2. **Missing required fields** - Ensure `name`, `version`, `description`, `plugin_type` exist
3. **Invalid version format** - Use semver: `1.0.0` not `1.0` or `v1.0.0`
4. **Invalid port numbers** - Must be 1-65535
5. **Unknown services** - Service must exist in registry

### Plugin Not Applied

**Problem:** `vm config preset my-plugin` doesn't work

**Solutions:**
1. Check plugin is installed: `vm plugin list`
2. Verify plugin type is "preset"
3. Check for errors: `vm config preset my-plugin --debug`
4. Validate preset content: `vm plugin validate my-plugin`

### Permission Issues

**Problem:** Cannot install plugin

**Solution:**
```bash
# Ensure ~/.vm/plugins directory exists and is writable
mkdir -p ~/.vm/plugins/presets
chmod 755 ~/.vm/plugins
```

---

## Examples

### Full-Stack JavaScript Preset

**plugin.yaml:**
```yaml
name: fullstack-js
version: 1.0.0
description: Modern full-stack JavaScript development environment
author: Jane Developer
plugin_type: preset
homepage: https://github.com/jane/vm-plugin-fullstack-js
license: MIT
tags:
  - javascript
  - fullstack
  - nodejs
```

**preset.yaml:**
```yaml
npm_packages:
  - typescript
  - tsx
  - eslint
  - prettier
  - vite
  - vitest

services:
  - postgresql
  - redis

environment:
  NODE_ENV: development
  DATABASE_URL: postgresql://localhost/app_dev
  REDIS_URL: redis://localhost:6379

aliases:
  dev: npm run dev
  test: npm test
  typecheck: tsc --noEmit
  lint: eslint .
```

### Custom Database Service

**plugin.yaml:**
```yaml
name: cockroachdb
version: 1.0.0
description: CockroachDB distributed SQL database
author: John DBA
plugin_type: service
homepage: https://www.cockroachlabs.com
license: Apache-2.0
```

**service.yaml:**
```yaml
service:
  name: cockroachdb
  display_name: CockroachDB
  description: Distributed SQL database compatible with PostgreSQL
  port: 26257
  health_endpoint: http://localhost:8080/health
  supports_graceful_shutdown: true
```

---

## FAQ

### Can plugins override embedded presets?

Yes. Plugin presets have higher priority than embedded presets. If you install a plugin named "nodejs", it will override the embedded nodejs preset.

### Where are plugins stored?

User plugins: `~/.vm/plugins/`
Embedded presets: Built into the vm binary
File-based presets: `configs/presets/`

### Can I share plugins?

Yes! Plugins are just directories. You can:
1. Copy the directory to another machine
2. Share via git repository
3. Package as tarball/zip

In the future, we may add a plugin registry for easier sharing.

### Do plugins work offline?

Yes. Once installed, plugins are stored locally and don't require internet access.

### Can plugins conflict?

Yes. If two plugins define the same service or have conflicting packages, the last applied preset wins. Review plugin contents before combining multiple presets.

### How do I update a plugin?

Currently:
1. Remove old version: `vm plugin remove <name>`
2. Install new version: `vm plugin install <path>`

In the future, we may add `vm plugin update` command.

### Can I disable a plugin temporarily?

Currently, you must remove it. A future enhancement may add `vm plugin disable/enable` commands.

---

## Advanced Topics

### Plugin Inheritance (Future)

Future versions may support plugin inheritance:

```yaml
# Advanced feature (not yet implemented)
extends: nodejs
npm_packages:
  - additional-package
```

### Plugin Dependencies (Future)

Future versions may support plugin dependencies:

```yaml
# Advanced feature (not yet implemented)
depends_on:
  - nodejs
  - docker
```

### Plugin Registry (Future)

Future versions may include a central registry:

```bash
# Advanced feature (not yet implemented)
vm plugin search react
vm plugin install @registry/react-typescript
```

---

## See Also

- [Presets Guide](./presets.md) - Using presets
- [CLI Reference](./cli-reference.md) - Command reference
- [Configuration Guide](./configuration.md) - VM configuration
