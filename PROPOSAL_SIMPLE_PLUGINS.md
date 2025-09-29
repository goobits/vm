# Proposal: Simple File-Based Plugin System (No-Brainer Edition)

## Executive Summary

Add a minimal plugin system using file-based discovery and YAML definitions. No dynamic loading, no complex architecture—just structured files in `~/.vm/plugins/` that extend presets and services. This is a **2-week implementation** using existing infrastructure.

## Why This Approach

**Leverage what exists:**
- Presets are already YAML files in `/configs/presets/`
- Service registry exists at `rust/vm/src/service_registry.rs`
- YAML parsing via `serde_yaml_ng` is battle-tested
- File discovery via `glob` crate is already used

**No complexity:**
- No dynamic library loading
- No sandboxing or permission models
- No plugin marketplace (yet)
- No runtime code execution

**Immediate value:**
- Users create custom presets without PR
- Organizations share internal presets via git
- Community can publish preset collections
- Foundation for future dynamic plugins

## Architecture

### Directory Structure

```
~/.vm/
├── plugins/
│   ├── presets/
│   │   ├── elixir/
│   │   │   ├── plugin.yaml        # Plugin metadata
│   │   │   └── preset.yaml        # Preset configuration
│   │   ├── haskell/
│   │   │   ├── plugin.yaml
│   │   │   └── preset.yaml
│   │   └── company-internal/
│   │       ├── plugin.yaml
│   │       └── preset.yaml
│   └── services/
│       ├── grafana/
│       │   ├── plugin.yaml        # Service metadata
│       │   └── service.yaml       # Service definition
│       └── prometheus/
│           ├── plugin.yaml
│           └── service.yaml
├── auth/
├── packages/
└── registry/
```

### Plugin Metadata Format

**`plugin.yaml`** - Universal plugin metadata:
```yaml
plugin:
  name: "elixir-phoenix"
  version: "1.0.0"
  type: "preset"  # or "service"
  author: "Community"
  description: "Elixir/Phoenix development environment with PostgreSQL"
  homepage: "https://github.com/username/vm-preset-elixir"
  license: "MIT"
  vm_version: ">=0.1.0"  # Minimum VM tool version
  tags: ["elixir", "phoenix", "web"]
```

### Preset Plugin Format

**`preset.yaml`** - Identical to existing preset format:
```yaml
---
preset:
  name: elixir-phoenix
  description: "Elixir/Phoenix development with PostgreSQL and Redis"

apt_packages:
  - erlang
  - elixir
  - inotify-tools

ports:
  http: "${port.0}"
  https: "${port.1}"

services:
  postgresql:
    enabled: true
    version: "15"
  redis:
    enabled: false

environment:
  MIX_ENV: "dev"
  DATABASE_URL: "postgresql://postgres:postgres@localhost:5432/dev"

detection:
  files:
    - "mix.exs"
    - "config/config.exs"
  directories:
    - "lib"
    - "deps"
  content_patterns:
    - "defmodule.*Phoenix"
```

### Service Plugin Format

**`service.yaml`** - Service definition:
```yaml
---
service:
  name: "grafana"
  display_name: "Grafana"
  description: "Metrics visualization and monitoring dashboard"
  port: 3100
  health_endpoint: "/api/health"
  supports_graceful_shutdown: true

container:
  image: "grafana/grafana:latest"
  ports:
    - "${port}:3000"
  volumes:
    - "${data_dir}/grafana:/var/lib/grafana"
  environment:
    GF_SECURITY_ADMIN_PASSWORD: "admin"
    GF_INSTALL_PLUGINS: "grafana-clock-panel"

dependencies:
  - "prometheus"  # Optional service dependency

configuration_schema:
  admin_password:
    type: "string"
    default: "admin"
    description: "Admin user password"
  plugins:
    type: "array"
    default: []
    description: "List of plugins to install"
```

## Implementation Plan

### Week 1: Core Infrastructure

#### Day 1-2: Plugin Discovery (`rust/vm-plugin/src/discovery.rs`)

```rust
use std::path::{Path, PathBuf};
use glob::glob;
use serde::{Deserialize, Serialize};

/// Plugin metadata from plugin.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub plugin: PluginInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub plugin_type: String,  // "preset" or "service"
    pub author: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub license: String,
    pub vm_version: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Discovered plugin with paths
#[derive(Debug, Clone)]
pub struct Plugin {
    pub metadata: PluginMetadata,
    pub plugin_dir: PathBuf,
    pub plugin_file: PathBuf,
    pub content_file: PathBuf,  // preset.yaml or service.yaml
}

/// Plugin discovery system
pub struct PluginDiscovery {
    plugins_dir: PathBuf,
}

impl PluginDiscovery {
    pub fn new() -> Self {
        let plugins_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".vm")
            .join("plugins");

        Self { plugins_dir }
    }

    /// Discover all plugins
    pub fn discover_all(&self) -> Result<Vec<Plugin>> {
        let mut plugins = Vec::new();

        // Search presets
        plugins.extend(self.discover_type("presets")?);

        // Search services
        plugins.extend(self.discover_type("services")?);

        Ok(plugins)
    }

    /// Discover plugins of a specific type
    fn discover_type(&self, plugin_type: &str) -> Result<Vec<Plugin>> {
        let type_dir = self.plugins_dir.join(plugin_type);
        if !type_dir.exists() {
            return Ok(Vec::new());
        }

        let mut plugins = Vec::new();
        let pattern = type_dir.join("*/plugin.yaml");

        for entry in glob(&pattern.to_string_lossy())?.flatten() {
            match self.load_plugin(&entry) {
                Ok(plugin) => plugins.push(plugin),
                Err(e) => {
                    eprintln!("⚠️  Failed to load plugin at {:?}: {}", entry, e);
                }
            }
        }

        Ok(plugins)
    }

    /// Load a single plugin from plugin.yaml path
    fn load_plugin(&self, plugin_file: &Path) -> Result<Plugin> {
        let plugin_dir = plugin_file.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid plugin path"))?
            .to_path_buf();

        // Parse plugin.yaml
        let content = std::fs::read_to_string(plugin_file)?;
        let metadata: PluginMetadata = serde_yaml::from_str(&content)?;

        // Determine content file based on type
        let content_filename = match metadata.plugin.plugin_type.as_str() {
            "preset" => "preset.yaml",
            "service" => "service.yaml",
            _ => return Err(anyhow::anyhow!("Unknown plugin type: {}", metadata.plugin.plugin_type)),
        };

        let content_file = plugin_dir.join(content_filename);
        if !content_file.exists() {
            return Err(anyhow::anyhow!("Missing {}", content_filename));
        }

        Ok(Plugin {
            metadata,
            plugin_dir,
            plugin_file: plugin_file.to_path_buf(),
            content_file,
        })
    }

    /// Get plugins by type
    pub fn get_by_type(&self, plugin_type: &str) -> Result<Vec<Plugin>> {
        Ok(self.discover_all()?
            .into_iter()
            .filter(|p| p.metadata.plugin.plugin_type == plugin_type)
            .collect())
    }

    /// Get plugin by name
    pub fn get_by_name(&self, name: &str) -> Result<Option<Plugin>> {
        Ok(self.discover_all()?
            .into_iter()
            .find(|p| p.metadata.plugin.name == name))
    }
}
```

#### Day 3-4: Preset Plugin Integration

Update `rust/vm-config/src/preset.rs`:

```rust
impl PresetDetector {
    /// Load a preset configuration by name
    pub fn load_preset(&self, name: &str) -> Result<VmConfig> {
        // Try embedded presets first
        if let Some(content) = crate::embedded_presets::get_preset_content(name) {
            let preset_file: PresetFile = serde_yaml::from_str(content)?;
            return Ok(preset_file.config);
        }

        // NEW: Check user plugins
        if let Ok(Some(plugin)) = self.load_plugin_preset(name) {
            return Ok(plugin);
        }

        // Fallback to file system (legacy)
        let preset_path = self.presets_dir.join(format!("{}.yaml", name));
        if !preset_path.exists() {
            return Err(VmError::Config(format!("Preset '{}' not found", name)));
        }

        let content = std::fs::read_to_string(&preset_path)?;
        let preset_file: PresetFile = serde_yaml::from_str(&content)?;
        Ok(preset_file.config)
    }

    /// Load preset from user plugins
    fn load_plugin_preset(&self, name: &str) -> Result<Option<VmConfig>> {
        let discovery = vm_plugin::PluginDiscovery::new();

        if let Some(plugin) = discovery.get_by_name(name)? {
            if plugin.metadata.plugin.plugin_type == "preset" {
                let content = std::fs::read_to_string(&plugin.content_file)?;
                let preset_file: PresetFile = serde_yaml::from_str(&content)?;
                return Ok(Some(preset_file.config));
            }
        }

        Ok(None)
    }

    /// Get list of available presets (including plugins)
    pub fn list_presets(&self) -> Result<Vec<String>> {
        let mut presets = Vec::new();

        // Add embedded presets
        for name in crate::embedded_presets::get_preset_names() {
            presets.push(name.to_string());
        }

        // NEW: Add plugin presets
        let discovery = vm_plugin::PluginDiscovery::new();
        for plugin in discovery.get_by_type("preset")? {
            presets.push(plugin.metadata.plugin.name.clone());
        }

        // Add file system presets (if presets dir exists)
        if self.presets_dir.exists() {
            // ... existing code ...
        }

        // Deduplicate (plugins override built-ins)
        presets.sort();
        presets.dedup();
        Ok(presets)
    }
}
```

#### Day 5: Service Plugin Integration

Update `rust/vm/src/service_registry.rs`:

```rust
impl ServiceRegistry {
    /// Create a new service registry with default + plugin services
    pub fn new() -> Self {
        let mut services = HashMap::new();

        // Built-in services (existing code)
        services.insert("auth_proxy".to_string(), /* ... */);
        services.insert("docker_registry".to_string(), /* ... */);
        services.insert("package_registry".to_string(), /* ... */);

        // NEW: Load plugin services
        if let Ok(plugin_services) = Self::load_plugin_services() {
            services.extend(plugin_services);
        }

        Self { services }
    }

    /// Load service definitions from plugins
    fn load_plugin_services() -> Result<HashMap<String, ServiceDefinition>> {
        let discovery = vm_plugin::PluginDiscovery::new();
        let mut services = HashMap::new();

        for plugin in discovery.get_by_type("service")? {
            match Self::load_service_from_plugin(&plugin) {
                Ok(service) => {
                    services.insert(plugin.metadata.plugin.name.clone(), service);
                }
                Err(e) => {
                    eprintln!("⚠️  Failed to load service plugin {}: {}",
                             plugin.metadata.plugin.name, e);
                }
            }
        }

        Ok(services)
    }

    /// Parse service.yaml into ServiceDefinition
    fn load_service_from_plugin(plugin: &Plugin) -> Result<ServiceDefinition> {
        let content = std::fs::read_to_string(&plugin.content_file)?;
        let service_config: ServiceConfig = serde_yaml::from_str(&content)?;

        Ok(ServiceDefinition {
            name: service_config.service.name,
            display_name: service_config.service.display_name,
            description: service_config.service.description,
            port: service_config.service.port,
            health_endpoint: service_config.service.health_endpoint,
            supports_graceful_shutdown: service_config.service.supports_graceful_shutdown,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ServiceConfig {
    service: ServiceInfo,
}

#[derive(Debug, Deserialize)]
struct ServiceInfo {
    name: String,
    display_name: String,
    description: String,
    port: u16,
    health_endpoint: String,
    supports_graceful_shutdown: bool,
}
```

### Week 2: CLI Commands & UX

#### Day 6-7: Plugin Management Commands

Add to `rust/vm/src/commands/mod.rs`:

```rust
pub mod plugin;
```

Create `rust/vm/src/commands/plugin.rs`:

```rust
use vm_plugin::PluginDiscovery;
use vm_core::{vm_println, vm_error};

/// Handle plugin list command
pub fn handle_plugin_list(plugin_type: Option<&str>) -> Result<()> {
    let discovery = PluginDiscovery::new();
    let plugins = match plugin_type {
        Some(ptype) => discovery.get_by_type(ptype)?,
        None => discovery.discover_all()?,
    };

    if plugins.is_empty() {
        vm_println!("No plugins installed.");
        vm_println!("\n💡 Install plugins with: vm plugin install <path>");
        return Ok(());
    }

    vm_println!("📦 Installed Plugins:\n");
    for plugin in plugins {
        let meta = &plugin.metadata.plugin;
        vm_println!("  {} v{}", meta.name, meta.version);
        vm_println!("    Type: {}", meta.plugin_type);
        vm_println!("    Author: {}", meta.author);
        vm_println!("    Description: {}", meta.description);
        if let Some(ref homepage) = meta.homepage {
            vm_println!("    Homepage: {}", homepage);
        }
        vm_println!();
    }

    Ok(())
}

/// Handle plugin info command
pub fn handle_plugin_info(name: &str) -> Result<()> {
    let discovery = PluginDiscovery::new();

    match discovery.get_by_name(name)? {
        Some(plugin) => {
            let meta = &plugin.metadata.plugin;
            vm_println!("📦 Plugin: {}\n", meta.name);
            vm_println!("  Version: {}", meta.version);
            vm_println!("  Type: {}", meta.plugin_type);
            vm_println!("  Author: {}", meta.author);
            vm_println!("  License: {}", meta.license);
            vm_println!("  Description: {}", meta.description);
            if let Some(ref homepage) = meta.homepage {
                vm_println!("  Homepage: {}", homepage);
            }
            vm_println!("  Minimum VM Version: {}", meta.vm_version);
            if !meta.tags.is_empty() {
                vm_println!("  Tags: {}", meta.tags.join(", "));
            }
            vm_println!("\n  📁 Location: {}", plugin.plugin_dir.display());
            Ok(())
        }
        None => {
            vm_error!("Plugin '{}' not found", name);
            Err(anyhow::anyhow!("Plugin not found"))
        }
    }
}

/// Handle plugin install command (copy from path)
pub fn handle_plugin_install(source_path: &Path) -> Result<()> {
    let discovery = PluginDiscovery::new();

    // Validate source is a plugin directory
    let plugin_yaml = source_path.join("plugin.yaml");
    if !plugin_yaml.exists() {
        return Err(anyhow::anyhow!("Not a valid plugin directory (missing plugin.yaml)"));
    }

    // Parse metadata to determine type and name
    let content = std::fs::read_to_string(&plugin_yaml)?;
    let metadata: PluginMetadata = serde_yaml::from_str(&content)?;

    let plugin_type = &metadata.plugin.plugin_type;
    let plugin_name = &metadata.plugin.name;

    // Determine destination
    let dest_dir = discovery.plugins_dir
        .join(format!("{}s", plugin_type))  // "preset" -> "presets"
        .join(plugin_name);

    if dest_dir.exists() {
        vm_println!("⚠️  Plugin '{}' already exists. Overwriting...", plugin_name);
        std::fs::remove_dir_all(&dest_dir)?;
    }

    // Create destination directory
    std::fs::create_dir_all(&dest_dir)?;

    // Copy all files from source to destination
    copy_dir_recursive(source_path, &dest_dir)?;

    vm_println!("✓ Plugin '{}' installed successfully", plugin_name);
    vm_println!("  Type: {}", plugin_type);
    vm_println!("  Location: {}", dest_dir.display());

    Ok(())
}

/// Recursively copy directory contents
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
```

#### Day 8-9: Plugin Template Generator

Create `rust/vm/src/commands/plugin_new.rs`:

```rust
use std::path::Path;

/// Generate a new plugin from template
pub fn handle_plugin_new(name: &str, plugin_type: &str) -> Result<()> {
    let plugin_dir = Path::new(name);

    if plugin_dir.exists() {
        return Err(anyhow::anyhow!("Directory '{}' already exists", name));
    }

    std::fs::create_dir_all(plugin_dir)?;

    match plugin_type {
        "preset" => generate_preset_template(plugin_dir, name)?,
        "service" => generate_service_template(plugin_dir, name)?,
        _ => return Err(anyhow::anyhow!("Unknown plugin type: {}", plugin_type)),
    }

    vm_println!("✓ Plugin template created: {}", name);
    vm_println!("\nNext steps:");
    vm_println!("  1. cd {}", name);
    vm_println!("  2. Edit plugin.yaml and {}.yaml", plugin_type);
    vm_println!("  3. vm plugin install .");

    Ok(())
}

fn generate_preset_template(dir: &Path, name: &str) -> Result<()> {
    // Create plugin.yaml
    let plugin_yaml = format!(r#"plugin:
  name: "{}"
  version: "1.0.0"
  type: "preset"
  author: "Your Name"
  description: "Custom preset for {}"
  license: "MIT"
  vm_version: ">=0.1.0"
  tags: []
"#, name, name);
    std::fs::write(dir.join("plugin.yaml"), plugin_yaml)?;

    // Create preset.yaml
    let preset_yaml = format!(r#"---
preset:
  name: {}
  description: "Custom development environment"

apt_packages: []
npm_packages: []
pip_packages: []
cargo_packages: []

ports:
  app: "${{port.0}}"

services:
  postgresql:
    enabled: false
  redis:
    enabled: false

environment:
  ENV: "development"

# Optional: Auto-detection configuration
detection:
  files: []
  directories: []
  content_patterns: []
"#, name);
    std::fs::write(dir.join("preset.yaml"), preset_yaml)?;

    Ok(())
}

fn generate_service_template(dir: &Path, name: &str) -> Result<()> {
    // Create plugin.yaml
    let plugin_yaml = format!(r#"plugin:
  name: "{}"
  version: "1.0.0"
  type: "service"
  author: "Your Name"
  description: "Custom service for {}"
  license: "MIT"
  vm_version: ">=0.1.0"
  tags: []
"#, name, name);
    std::fs::write(dir.join("plugin.yaml"), plugin_yaml)?;

    // Create service.yaml
    let service_yaml = format!(r#"---
service:
  name: "{}"
  display_name: "{}"
  description: "Custom managed service"
  port: 3100
  health_endpoint: "/health"
  supports_graceful_shutdown: true

container:
  image: "your-image:latest"
  ports:
    - "${{port}}:8080"
  volumes:
    - "${{data_dir}}/{}:/data"
  environment:
    LOG_LEVEL: "info"

dependencies: []
"#, name, name, name);
    std::fs::write(dir.join("service.yaml"), service_yaml)?;

    Ok(())
}
```

#### Day 10: CLI Integration

Update `rust/vm/src/cli.rs` to add plugin commands:

```rust
#[derive(Debug, Parser)]
pub enum Command {
    // ... existing commands ...

    /// Manage plugins
    #[command(subcommand)]
    Plugin(PluginCommand),
}

#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// List installed plugins
    List {
        /// Filter by plugin type (preset, service)
        #[arg(long)]
        r#type: Option<String>,
    },

    /// Show plugin details
    Info {
        /// Plugin name
        name: String,
    },

    /// Install plugin from path
    Install {
        /// Path to plugin directory
        path: PathBuf,
    },

    /// Create new plugin from template
    New {
        /// Plugin name
        name: String,

        /// Plugin type (preset, service)
        #[arg(long, default_value = "preset")]
        r#type: String,
    },
}
```

Update command handler:

```rust
match args.command {
    Command::Plugin(plugin_cmd) => {
        match plugin_cmd {
            PluginCommand::List { r#type } => {
                plugin::handle_plugin_list(r#type.as_deref())
            }
            PluginCommand::Info { name } => {
                plugin::handle_plugin_info(&name)
            }
            PluginCommand::Install { path } => {
                plugin::handle_plugin_install(&path)
            }
            PluginCommand::New { name, r#type } => {
                plugin::handle_plugin_new(&name, &r#type)
            }
        }
    }
    // ... existing commands ...
}
```

## User Experience

### Creating a Custom Preset

```bash
# Generate template
vm plugin new elixir-phoenix --type preset

# Edit files
cd elixir-phoenix
nano plugin.yaml  # Add metadata
nano preset.yaml  # Configure environment

# Install locally
vm plugin install .

# Use it
cd ~/my-elixir-project
vm create  # Auto-detects and uses elixir-phoenix preset
```

### Sharing Presets

```bash
# Create git repo
cd elixir-phoenix
git init
git add .
git commit -m "Initial Elixir preset"
git remote add origin https://github.com/username/vm-preset-elixir.git
git push -u origin main

# Others can install via git clone
git clone https://github.com/username/vm-preset-elixir.git ~/.vm-presets/elixir
vm plugin install ~/.vm-presets/elixir
```

### Organization Presets

```bash
# Company shares internal preset via git
git clone https://github.com/company/vm-preset-internal.git
vm plugin install vm-preset-internal

# Or mount shared NFS/network drive
ln -s /mnt/shared/vm-presets/company ~/.vm/plugins/presets/company
```

### Listing Plugins

```bash
vm plugin list
# Output:
# 📦 Installed Plugins:
#
#   elixir-phoenix v1.0.0
#     Type: preset
#     Author: Community
#     Description: Elixir/Phoenix development with PostgreSQL
#
#   company-internal v2.1.0
#     Type: preset
#     Author: ACME Corp
#     Description: Internal development stack

vm plugin list --type preset  # Filter by type
vm plugin info elixir-phoenix # Detailed info
```

## Migration Path

### Phase 1: Foundation (This Proposal)
- File-based plugins only
- Preset and service plugins
- Basic CLI commands
- Zero breaking changes

### Phase 2: Enhanced Discovery (Future)
- Plugin registry/marketplace
- `vm plugin search <query>`
- `vm plugin install <name>` (from registry)
- Version management

### Phase 3: Dynamic Plugins (Future)
- Dynamic library loading
- Provider plugins
- Lifecycle hooks
- Full plugin API (from original proposal)

## Crate Structure

```
rust/
├── vm-plugin/              # NEW: Plugin system core
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── discovery.rs    # File-based discovery
│       ├── types.rs        # PluginMetadata, Plugin structs
│       └── validation.rs   # Plugin validation
├── vm/
│   └── src/
│       └── commands/
│           └── plugin.rs   # NEW: Plugin CLI commands
├── vm-config/              # MODIFIED: Preset loading
└── vm/src/service_registry.rs  # MODIFIED: Service loading
```

## Benefits

1. **Zero Learning Curve** - YAML files, existing preset format
2. **Immediate Value** - Custom presets without waiting for PRs
3. **Simple Distribution** - Git clone, file copy, or network mount
4. **No Security Risks** - No code execution, just data files
5. **Fast Implementation** - 2 weeks, leverages existing code
6. **Foundation** - Enables future dynamic plugin system
7. **Backward Compatible** - Existing presets work unchanged

## Testing

### Unit Tests
- Plugin discovery and parsing
- Metadata validation
- Preset/service loading from plugins
- CLI command handlers

### Integration Tests
- Install plugin and use preset
- List plugins with various filters
- Create plugin from template
- Plugin overriding built-in preset

### User Acceptance
- Community creates 3+ custom presets
- Documentation with examples
- Zero bug reports on plugin loading

## Documentation

### User Guide
- `docs/plugins/README.md` - Overview
- `docs/plugins/creating-presets.md` - Preset plugin guide
- `docs/plugins/creating-services.md` - Service plugin guide
- `docs/plugins/sharing.md` - Distribution methods

### Examples
- `examples/plugins/elixir-preset/` - Complete preset example
- `examples/plugins/custom-service/` - Service plugin example

## Success Metrics

- **Implementation**: 2 weeks (10 working days)
- **Community Adoption**: 5+ custom presets within 1 month
- **Zero Breaking Changes**: All existing functionality works
- **Performance**: <10ms overhead for plugin discovery
- **Reliability**: 100% of valid plugins load successfully

## Conclusion

This minimal plugin system provides immediate value with minimal implementation cost. By reusing existing infrastructure (YAML parsing, file discovery, service registry), we enable community extensibility in just 2 weeks. This serves as a solid foundation for future enhancements while maintaining the tool's zero-config philosophy.