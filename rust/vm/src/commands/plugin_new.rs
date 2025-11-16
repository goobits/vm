use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use vm_cli::msg;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;

pub fn handle_plugin_new(plugin_name: &str, plugin_type: &str) -> Result<()> {
    // Validate plugin name
    if plugin_name.is_empty() {
        anyhow::bail!("Plugin name cannot be empty");
    }

    if !plugin_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!(
            "Plugin name must contain only alphanumeric characters, hyphens, and underscores"
        );
    }

    // Validate and parse plugin type
    let plugin_type_lower = plugin_type.to_lowercase();
    if plugin_type_lower != "preset" && plugin_type_lower != "service" {
        anyhow::bail!("Invalid plugin type '{plugin_type}'. Must be either 'preset' or 'service'");
    }

    let plugin_dir = PathBuf::from(plugin_name);

    if plugin_dir.exists() {
        anyhow::bail!("Directory '{plugin_name}' already exists");
    }

    // Create plugin directory
    fs::create_dir_all(&plugin_dir).context("Failed to create plugin directory")?;

    // Create plugin.yaml (metadata)
    let metadata_content = generate_metadata_template(plugin_name, &plugin_type_lower);
    fs::write(plugin_dir.join("plugin.yaml"), metadata_content)
        .context("Failed to create plugin.yaml")?;

    // Create content file based on type
    if plugin_type_lower == "preset" {
        let preset_content = generate_preset_template();
        fs::write(plugin_dir.join("preset.yaml"), preset_content)
            .context("Failed to create preset.yaml")?;
    } else {
        let service_content = generate_service_template();
        fs::write(plugin_dir.join("service.yaml"), service_content)
            .context("Failed to create service.yaml")?;
    }

    // Create README.md
    let readme_content = generate_readme_template(plugin_name, &plugin_type_lower);
    fs::write(plugin_dir.join("README.md"), readme_content)
        .context("Failed to create README.md")?;

    let type_cap = match plugin_type_lower.as_str() {
        "preset" => "Preset",
        "service" => "Service",
        _ => "Plugin",
    };

    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin.new_success,
            r#type = &plugin_type_lower,
            name = plugin_name
        )
    );
    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin.new_next_steps,
            name = plugin_name,
            r#type = &plugin_type_lower
        )
    );
    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin.new_files_created,
            r#type = &plugin_type_lower,
            type_cap = type_cap
        )
    );

    Ok(())
}

fn generate_metadata_template(plugin_name: &str, plugin_type: &str) -> String {
    format!(
        r#"# Plugin metadata for {plugin_name}
name: {plugin_name}
version: 0.1.0
description: A custom VM {plugin_type} plugin
author: Your Name
plugin_type: {plugin_type}
"#
    )
}

fn generate_preset_template() -> String {
    r#"# Preset configuration
# Define packages, services, environment variables, and provisioning steps

# System packages to install via apt/yum
packages:
  - curl
  - git
  - build-essential

# NPM packages to install globally
npm_packages:
  - typescript
  - prettier

# Python packages to install
pip_packages:
  - black
  - pylint

# Rust packages to install
cargo_packages:
  - cargo-watch

# Services to enable (must be defined in service plugins or built-in)
services:
  - postgres
  - redis

# Environment variables
environment:
  NODE_ENV: development
  RUST_LOG: debug

# Shell aliases
aliases:
  ll: ls -la
  gst: git status

# Provisioning commands (run during VM setup)
provision:
  - echo "Running custom provisioning"
  - echo "Add your setup commands here"
"#
    .to_string()
}

fn generate_service_template() -> String {
    r#"# Service configuration
# Define a Docker service that can be referenced by presets

# Docker image to use
image: redis:7-alpine

# Port mappings (host:container format)
ports:
  - "6379:6379"

# Volume mappings
volumes:
  - "redis_data:/data"

# Environment variables
environment:
  REDIS_PASSWORD: changeme

# Command to run (optional, overrides image CMD)
# command:
#   - redis-server
#   - --appendonly
#   - "yes"

# Service dependencies (start order)
depends_on:
  []

# Health check endpoint (optional, for service registry)
health_check: /health
"#
    .to_string()
}

fn generate_readme_template(plugin_name: &str, plugin_type: &str) -> String {
    if plugin_type == "preset" {
        format!(
            r#"# {plugin_name}

A custom preset plugin for VM Tool.

## Description

This preset provides a development environment with pre-configured packages, services, and settings.

## Installation

```bash
vm plugin install /path/to/{plugin_name}
```

## Usage

Create a VM using this preset:

```bash
vm create my-project --preset {plugin_name}
```

Or add to your `vm.yaml`:

```yaml
name: my-project
preset: {plugin_name}
```

## What's Included

### Packages
- System packages: curl, git, build-essential
- NPM packages: typescript, prettier
- Python packages: black, pylint
- Rust packages: cargo-watch

### Services
- PostgreSQL database
- Redis cache

### Environment
- `NODE_ENV=development`
- `RUST_LOG=debug`

### Aliases
- `ll` → `ls -la`
- `gst` → `git status`

## Customization

Edit `preset.yaml` to customize:
- Packages to install
- Services to enable
- Environment variables
- Shell aliases
- Provisioning commands

## License

MIT
"#
        )
    } else {
        format!(
            r#"# {plugin_name}

A custom service plugin for VM Tool.

## Description

This service plugin provides a containerized service that can be used by VM presets.

## Installation

```bash
vm plugin install /path/to/{plugin_name}
```

## Usage

Reference this service in a preset or `vm.yaml`:

```yaml
name: my-project
services:
  - {plugin_name}
```

## Configuration

The service uses:
- **Image**: redis:7-alpine
- **Port**: 6379
- **Volume**: redis_data:/data

### Environment Variables
- `REDIS_PASSWORD`: Authentication password (default: "changeme")

## Customization

Edit `service.yaml` to customize:
- Docker image and version
- Port mappings
- Volume mounts
- Environment variables
- Command to run
- Service dependencies

## Health Check

The service provides a health check endpoint at `/health` for monitoring.

## License

MIT
"#
        )
    }
}
