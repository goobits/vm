# 🏗️ Architecture Overview

Understanding the VM development environment's architecture and design principles.

## 🎯 High-Level Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   User Input    │    │   Configuration  │    │    Providers    │
│                 │    │                  │    │                 │
│ • CLI Commands  │───▶│ • Auto-detection │───▶│ • Docker        │
│ • vm.yaml       │    │ • Preset System  │    │ • Vagrant       │
│ • Presets       │    │ • Validation     │    │ • Tart          │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                │
                                ▼
                       ┌──────────────────┐
                       │   VM/Container   │
                       │                  │
                       │ • Services       │
                       │ • File Sync      │
                       │ • Port Forward   │
                       │ • Environment    │
                       └──────────────────┘
```

## 📁 Core Components

### 1. Main Entry Point (`vm` wrapper → Rust binary)
**Purpose**: CLI interface and command routing
- Wrapper script delegates to compiled Rust binary (`rust/target/release/vm`)
- Rust binary handles argument parsing and command routing
- Manages configuration loading and validation
- Coordinates provider operations

**Key Components**:
- `vm` - Shell wrapper script
- `rust/src/main.rs` - Main Rust entry point
- `rust/vm-config/` - Configuration handling
- `rust/vm-provider/` - Provider abstraction

### 2. Configuration System (`shared/config-processor.sh`)
**Purpose**: YAML processing, validation, and preset application
- Loads and validates YAML configurations
- Applies preset layering based on project detection
- Merges user config with presets and defaults
- Handles configuration inheritance and overrides

**Key Functions**:
- `load_config_with_presets()` - Main config loading with preset logic
- `apply_presets()` - Preset application and merging
- `validate_config()` - Schema validation

### 3. Project Detection (`shared/project-detector.sh`)
**Purpose**: Automatic framework and technology detection
- Scans project files for framework indicators
- Returns detected technologies as preset suggestions
- Handles multi-technology projects

**Detection Logic**:
```bash
# Framework detection examples
detect_react()   → package.json + react dependency
detect_django()  → manage.py + Django patterns
detect_rails()   → Gemfile + Rails patterns
detect_docker()  → Dockerfile presence
```

### 4. Provider System (`shared/provider-interface.sh`)
**Purpose**: Abstraction layer for different virtualization providers
- Unified interface for Docker, Vagrant, and Tart
- Provider capability detection and selection
- Command delegation to provider-specific implementations

**Provider Interface**:
```bash
provider_create()   # Create VM/container
provider_start()    # Start existing VM
provider_stop()     # Stop running VM
provider_destroy()  # Delete VM completely
provider_ssh()      # SSH access
provider_exec()     # Execute commands
provider_status()   # Get status information
provider_logs()     # View logs
```

### 5. Individual Providers

#### Docker Provider (`providers/docker/`)
- **Use Case**: Lightweight containers, fast startup
- **Implementation**: Docker Compose with service orchestration
- **File Sync**: Volume mounts (native performance)
- **Services**: Containerized PostgreSQL, Redis, etc.

#### Vagrant Provider (`providers/vagrant/`)
- **Use Case**: Full VM isolation, maximum security
- **Implementation**: VirtualBox/VMware integration
- **File Sync**: Native folder sharing
- **Services**: System-level service installation

#### Tart Provider (`providers/tart/`)
- **Use Case**: Native virtualization on Apple Silicon
- **Implementation**: Direct Tart CLI integration
- **File Sync**: SSH-based synchronization
- **Services**: Both macOS and Linux VM support

### 6. Temporary VM System (`shared/temporary-vm-utils.sh`)
**Purpose**: Ephemeral development environments
- Quick directory-specific environments
- Dynamic mount management
- Isolated experimentation spaces

## 🔄 Command Flow

### VM Creation Flow
```
1. CLI Parsing (Rust binary via vm wrapper)
   └─ Command: "vm create"

2. Configuration Loading (Rust vm-config crate)
   ├─ Load user config (vm.yaml)
   ├─ Detect project type (Rust project detection)
   ├─ Apply presets based on detection
   ├─ Merge with defaults
   └─ Validate final configuration

3. Provider Selection (Rust vm-provider crate)
   ├─ Check provider availability
   ├─ Select best provider for config
   └─ Delegate to provider implementation

4. VM/Container Creation (provider-specific)
   ├─ Generate provider configs (Dockerfile/Vagrantfile)
   ├─ Set up networking and ports
   ├─ Configure file synchronization
   ├─ Install services and dependencies
   └─ Return connection information

5. Post-Creation (Rust binary)
   └─ Display connection information and next steps
```

### Preset Application Flow
```
1. Project Scanning (Rust project detection)
   ├─ Scan for package.json (Node.js indicators)
   ├─ Scan for requirements.txt (Python indicators)
   ├─ Scan for Gemfile (Ruby indicators)
   ├─ Scan for manage.py (Django indicators)
   ├─ Scan for Dockerfile (Docker indicators)
   └─ Return detected preset list

2. Preset Loading (Rust vm-config crate)
   ├─ Load preset YAML files
   ├─ Apply presets in priority order
   ├─ Merge with user configuration
   └─ Resolve conflicts (user config wins)

3. Configuration Finalization
   ├─ Apply environment-specific defaults
   ├─ Validate final configuration
   └─ Generate provider-specific configs
```

## 🧩 Design Principles

### 1. **Provider Abstraction**
- Uniform interface across Docker, Vagrant, and Tart
- Provider-specific optimizations hidden from users
- Easy to add new virtualization backends

### 2. **Configuration Layering**
```yaml
# Priority order (highest to lowest):
1. User vm.yaml          # Explicit user choices
2. CLI flags             # Command-line overrides
3. Applied presets       # Framework-specific defaults
4. System defaults       # Fallback values
```

### 3. **Zero-Configuration Philosophy**
- Work without any configuration file
- Intelligent defaults based on project detection
- Progressive disclosure of complexity

### 4. **Preset System Architecture**
```
Presets are additive and composable:
- Base preset provides core functionality
- Framework presets add specific tools/services
- Multiple presets can be combined for complex projects
- User config always takes precedence
```

### 5. **Modular Design**
- Each major feature is in its own module
- Clear interfaces between components
- Easy to test individual components
- Plugin architecture for extensibility

## ⚡ Performance Considerations

### Current Performance Bottlenecks
1. **Shell Script Processing**: Configuration loading and validation
2. **Sequential Operations**: Port checking and link detection
3. **File System Scanning**: Project detection across large repositories

### Ongoing Rust Migration
```
Performance-critical components being rewritten in Rust:

vm-config:  YAML processing          200ms → 20ms
vm-links:   Package link detection   500ms → 50ms
vm-ports:   Port management          200ms → 2ms
```

### Optimization Strategies
- **Lazy Loading**: Only scan when detection needed
- **Caching**: Cache detection results and configuration
- **Parallel Processing**: Concurrent provider operations
- **Native Tools**: Replace shell loops with compiled binaries

## 🔧 Extension Points

### Adding New Presets
1. Create preset YAML in `configs/presets/`
2. Add detection logic to `shared/project-detector.sh`
3. Add tests to `test/unit/preset-detection.test.sh`

### Adding New Providers
1. Create provider directory in `providers/`
2. Implement provider interface functions
3. Add provider detection to `shared/provider-interface.sh`
4. Add tests to `test/system/vm-lifecycle.test.sh`

### Adding New Services
1. Add service configuration to preset YAMLs
2. Update provider provisioning scripts
3. Add service-specific tests

## 🧪 Testing Architecture

### Test Categories
- **Unit Tests** (`test/unit/`): Individual function testing
- **Integration Tests** (`test/integration/`): Component interaction
- **System Tests** (`test/system/`): End-to-end workflows

### Test Strategy
- Mock external dependencies where possible
- Use real VMs/containers for system tests
- Validate configurations without creating VMs
- Test preset detection with temporary file structures

## 📊 Metrics and Monitoring

### Performance Metrics
- VM creation time
- Configuration processing time
- Memory usage during operation
- Disk space usage for VMs/containers

### Error Tracking
- Configuration validation failures
- Provider availability issues
- Service startup failures
- Network connectivity problems

## 🔮 Future Architecture

### Planned Improvements
1. **Plugin System**: External preset and provider plugins
2. **API Interface**: HTTP API for programmatic access
3. **State Management**: Persistent VM state tracking
4. **Cluster Support**: Multi-VM environments
5. **Resource Optimization**: Automatic resource scaling

### Migration Path
- Gradual Rust adoption for performance-critical components
- Maintain shell script compatibility during transition
- Add API layer without breaking CLI interface
- Preserve existing configuration format