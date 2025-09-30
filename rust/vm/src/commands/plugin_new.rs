use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub fn handle_plugin_new(plugin_name: &str) -> Result<()> {
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

    let plugin_dir = PathBuf::from(plugin_name);

    if plugin_dir.exists() {
        anyhow::bail!("Directory '{}' already exists", plugin_name);
    }

    // Create plugin directory
    fs::create_dir_all(&plugin_dir).context("Failed to create plugin directory")?;

    // Create plugin.yaml
    let manifest_content = generate_manifest_template(plugin_name);
    fs::write(plugin_dir.join("plugin.yaml"), manifest_content)
        .context("Failed to create plugin.yaml")?;

    // Create README.md
    let readme_content = generate_readme_template(plugin_name);
    fs::write(plugin_dir.join("README.md"), readme_content)
        .context("Failed to create README.md")?;

    // Create examples directory with sample files
    let examples_dir = plugin_dir.join("examples");
    fs::create_dir_all(&examples_dir).context("Failed to create examples directory")?;

    let example_preset = generate_example_preset_template();
    fs::write(examples_dir.join("example-preset.yaml"), example_preset)
        .context("Failed to create example preset")?;

    println!("✓ Created plugin template: {}", plugin_name);
    println!();
    println!("Next steps:");
    println!("  1. cd {}", plugin_name);
    println!("  2. Edit plugin.yaml to define your presets and services");
    println!("  3. Test your plugin: vm plugin install .");
    println!();
    println!("Documentation:");
    println!("  - plugin.yaml: Main plugin configuration");
    println!("  - examples/: Example configurations");
    println!("  - README.md: Plugin documentation");

    Ok(())
}

fn generate_manifest_template(plugin_name: &str) -> String {
    format!(
        r#"# Plugin manifest for {}
name: {}
version: 0.1.0
description: A custom VM plugin
author: Your Name

# Define presets (optional)
presets:
  - name: example
    description: Example preset demonstrating plugin features
    base_image: ubuntu:22.04
    packages:
      - curl
      - git
    services:
      - redis  # Reference to a service defined below
    env:
      EXAMPLE_VAR: "example_value"
    ports:
      - "8080:8080"
    volumes:
      - "./data:/data"
    provision:
      - echo "Running custom provisioning"
      - echo "Add your setup commands here"

# Define services (optional)
services:
  - name: redis
    description: Redis cache service
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - "redis_data:/data"
    env:
      REDIS_PASSWORD: "changeme"
    depends_on: []

  - name: postgres
    description: PostgreSQL database service
    image: postgres:15-alpine
    ports:
      - "5432:5432"
    volumes:
      - "postgres_data:/var/lib/postgresql/data"
    env:
      POSTGRES_USER: "myuser"
      POSTGRES_PASSWORD: "mypassword"
      POSTGRES_DB: "mydb"
    depends_on: []
"#,
        plugin_name, plugin_name
    )
}

fn generate_readme_template(plugin_name: &str) -> String {
    format!(
        r#"# {}

A custom plugin for VM Tool.

## Description

Add a detailed description of your plugin here.

## Installation

```bash
vm plugin install /path/to/{}
```

## Presets

### example

Example preset demonstrating plugin features.

**Usage:**

```bash
vm create my-project --preset example
```

**Features:**
- Ubuntu 22.04 base image
- Pre-installed: curl, git
- Includes Redis service
- Custom environment variables
- Port 8080 exposed

## Services

### redis

Redis cache service (v7 Alpine)

**Ports:** 6379
**Environment:**
- `REDIS_PASSWORD`: Redis authentication password

### postgres

PostgreSQL database service (v15 Alpine)

**Ports:** 5432
**Environment:**
- `POSTGRES_USER`: Database user
- `POSTGRES_PASSWORD`: Database password
- `POSTGRES_DB`: Database name

## Configuration

You can customize the plugin by editing `plugin.yaml`:

- **presets**: Define custom VM configurations
- **services**: Define reusable service containers

## Examples

See the `examples/` directory for sample configurations.

## License

MIT
"#,
        plugin_name, plugin_name
    )
}

fn generate_example_preset_template() -> String {
    r#"# Example: Using the plugin preset
#
# This file demonstrates how to use a preset from this plugin
# in a vm.yaml configuration file.

name: my-project

# Use the preset from the plugin
preset: example

# You can override or extend preset settings
env:
  CUSTOM_VAR: "custom_value"

# Add additional services beyond those in the preset
services:
  - mongodb

# Add custom provisioning steps
provision:
  - echo "Custom provisioning step"
"#
    .to_string()
}
