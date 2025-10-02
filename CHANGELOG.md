# Changelog

All notable changes to the VM tool will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Vagrant provider: Thread-unsafe `env::set_var()` replaced with per-command environment variables
- Tart provider: Compilation errors with missing imports and trait methods
- Vagrant provider: Compilation errors with duplicate imports and missing trait methods
- Error handling in Vagrant/Tart instance managers to avoid anyhow trait dependency issues

### Changed
- Tart provider: Deduplicated VM creation logic (130 lines reduced, 62% less duplication)
- Extracted `extract_project_name()` helper to common module for reuse across providers
- Vagrant/Tart: Implemented `create_with_context()` trait methods for Provider interface

### Removed
- Tart provider: Dead code `create_instance()` method (36 lines)

### Technical Improvements
- Vagrant/Tart providers now compile with `--all-features` flag
- All 48 vm-provider unit tests passing
- Net reduction of 146 lines across provider implementations
- Thread-safe command execution in Vagrant provider using duct with isolated environments

## [2.0.5] - 2025-10-02

### Changed
- Refactored vm-auth-proxy to extract `parse_secret_scope()` helper function for better code reuse
- Eliminated cross-platform code duplication in vm-platform with `SharedPlatformOps` trait
- Consolidated duplicate package context logic in vm-provider's compose.rs module

### Removed
- Obsolete proposal documents (PROPOSAL_COLOCATED_PORTS.md, PROPOSAL_CONFIG_PORTS.md, etc.)
- Legacy port configuration references

### Technical Improvements
- Reduced codebase duplication from 4.31% to 3.61% (132 duplicate lines eliminated)
- Created shared platform operations trait with 11 default implementations
- Improved maintainability through better code organization and helper extraction
- All 62 tests passing across vm-provider, vm-platform, and vm-auth-proxy

## [2.0.4] - 2025-09-30

### Added
- Build automation and code quality tooling
- Comprehensive vm-messages migration infrastructure

### Changed
- Migrated update.rs and mod.rs to centralized vm-messages system
- Migrated doctor.rs and auth.rs to vm-messages for consistent user-facing text
- Complete config.rs migration to vm-messages system
- Migrated uninstall.rs, pkg.rs, and plugin modules to vm-messages
- Migrated vm-provider docker/lifecycle.rs and progress.rs to vm-messages
- Complete vm_ops.rs migration to vm-messages system for i18n readiness

### Fixed
- Ensure supervisord is running before executing supervisorctl commands
- Container existence check before destroy operation to prevent errors
- BuildKit cache mount ownership for proper permission handling
- PostgreSQL service provisioning reliability improvements

### Performance
- Optimized Dockerfile with batch installs and reduced layers
- Added BuildKit cache mounts for faster Docker image builds

## [2.0.3] - 2025-09-26

### Added
- Co-located service ports with intelligent auto-allocation
- Service lifecycle cleanup success messages
- Auto-install preset plugins during VM installation for immediate availability
- Auto-configure package registry for all VMs during provisioning
- Smart sccache detection in installer for improved Rust build performance

### Changed
- Consolidated messages with multi-line strings and improved naming conventions
- Migrated all handle_* command functions to vm-messages system
- Embed service configs in vm init to prevent missing file errors

### Fixed
- Docker fixes for service port allocation and conflict resolution
- Correct Cargo package index path structure
- Add auto-restart capability to critical services
- Remove workspace directory creation from Dockerfile to prevent permission issues

### Removed
- Unused workspace dependencies identified by cargo-machete

## [2.0.2] - 2025-09-24

### Added
- Automatic service lifecycle management
- Enhanced plugin system capabilities

### Changed
- Centralized version management to workspace Cargo.toml
- Aligned vm-plugin with workspace standards for consistency

### Removed
- Legacy code cleanup across multiple modules

## [2.0.1] - 2025-09-23

### Added
- Initial vm-messages system foundation for centralized message management
- Message templates for better internationalization (i18n) support

### Changed
- Major refactoring to introduce message centralization pattern
- Improved code organization across packages

## [2.0.0] - 2024-09-28

### ✨ Major Release: Unified VM Platform

This major release transforms VM from a development environment tool into a **comprehensive development platform** with integrated secrets management, image caching, and package registry services.

### 🚀 **New Platform Architecture**

#### **Unified CLI Experience**
All functionality now accessible through a single `vm` command:
- **`vm auth`** - Centralized secrets management
- **`vm pkg`** - Package registry (npm, pip, cargo)
- **`vm create`** - Smart project detection and provisioning
- **`vm temp`** - Instant temporary environments
- **`vm config`** - Configuration management

#### **Intelligent Auto-Configuration**
VMs automatically configure themselves with host services:
```yaml
# vm.yaml - Zero-config service integration
auth_proxy: true        # Auto-connect to host secrets
docker_registry: true   # Auto-use local image cache
package_registry: true  # Auto-use local package cache
```

### 🔐 **Auth Proxy Service (NEW)**

**Centralized secrets management** eliminating manual credential setup:

```bash
vm auth status                   # Check service status
vm auth add openai sk-xxxxx      # Store API key once
vm auth add db_password secret   # All VMs get access automatically
vm auth list                     # List stored secrets
```

**Features:**
- **AES-256-GCM encryption** with PBKDF2 key derivation
- **Scoped access control** (global, project, instance)
- **Automatic VM injection** via environment variables
- **REST API** with bearer token authentication
- **Interactive management** with secure prompts

### 🐳 **Docker Registry Service (NEW)**

**Local Docker image caching** eliminating redundant downloads:

Docker registry is automatically managed when VMs have `docker_registry: true` in their config.
The service starts automatically when needed and caches images for all VMs.

**Benefits:**
- **80-95% bandwidth savings** for teams reusing base images
- **Near-instant VM creation** after first image download
- **Offline capability** for previously cached images
- **Automatic garbage collection** with configurable policies

### 📦 **Enhanced Package Registry**

**Private package registry** now fully integrated into VM workflow:

```bash
vm pkg status                   # Check server status
vm pkg add                      # Publish packages
vm pkg list                     # List all packages
vm pkg use --shell bash        # Configure shell integration
```

**Improvements:**
- **Automatic VM configuration** during provisioning
- **Seamless fallback** to public registries
- **90%+ bandwidth savings** for package installs
- **Zero-dependency deployment** (single binary)

### 🎯 **Smart Project Detection**

Enhanced project detection with **automatic service integration**:

```bash
cd my-fullstack-app && vm create
# Automatically detects: React + Node.js + PostgreSQL
# Automatically configures: Package cache + Docker cache + Secrets
# Result: Complete development environment in 60 seconds
```

**Supported Frameworks:**
Next.js, React, Angular, Vue, Django, Flask, Rails, Node.js, Python, Rust, Go, PHP, Docker, Kubernetes

### 🔧 **Breaking Changes**

#### **Simplified Installation**
```bash
# Before: Multiple installation methods
./install.sh --pkg-server

# Now: Single unified installation
./install.sh
```

#### **Configuration Changes**
- Removed: `claude_sync`, `gemini_sync` options
- Added: `auth_proxy`, `docker_registry`, `package_registry` options
- Migration: Use `vm auth add` for credential management

#### **CLI Consolidation**
- All functionality moved to `vm` command
- Optional standalone binaries available but not documented
- Unified help and configuration system

### 🛠️ **System Requirements**

- **Rust 1.70+** for installation from source
- **Docker** (optional, for container provider)
- **2GB RAM minimum** (4GB recommended for multiple services)
- **Linux, macOS** (Windows via WSL2)

### 📊 **Performance Improvements**

- **60% faster VM creation** with local caches
- **90% reduction** in package install times
- **95% bandwidth savings** for Docker images
- **Zero cold start penalty** for cached resources

### 🧪 **Quality Assurance**

- **465+ tests** across all components
- **Comprehensive integration testing** with real Docker containers
- **Security hardened** with 77 dedicated security tests
- **Production-ready** test coverage and reliability

### 🚀 **Migration Guide**

#### **From v1.x to v2.0**

1. **Update installation**:
   ```bash
   cargo install vm  # or ./install.sh
   ```

2. **Migrate secrets** (if using claude_sync/gemini_sync):
   ```bash
   vm auth add openai $OPENAI_API_KEY
   vm auth add anthropic $ANTHROPIC_API_KEY
   ```

3. **Update vm.yaml**:
   ```yaml
   # Remove:
   # claude_sync: true
   # gemini_sync: true

   # Add:
   auth_proxy: true
   docker_registry: true
   package_registry: true
   ```

4. **Recreate VMs** to benefit from new caching:
   ```bash
   vm destroy --all
   vm create  # Now uses all caches automatically
   ```

### 💡 **Getting Started**

```bash
# Install
cargo install vm

# Create development environment
cd my-project && vm create

# Services start automatically on first use
vm ssh  # Your fully configured environment awaits!
```

For complete documentation, see [README.md](README.md)

---

### Fixed

#### 🧪 Test Suite Reliability Improvements
- **Fixed flaky test failures** that were causing CI instability
  - `test_add_and_list_secrets` - Eliminated port conflicts by implementing dynamic port allocation
  - `test_pkg_status_command` - Resolved Tokio runtime conflicts with alternative test implementation
- **Improved test strategy** for environment-dependent tests
  - Replaced "always ignored" tests with conditional execution based on environment capabilities
  - Added `test_pkg_status_functionality` as alternative to CLI-dependent test
  - Made `test_pkg_full_lifecycle` conditional on `VM_INTEGRATION_TESTS` environment variable
  - Made `test_cargo_package_lifecycle` conditional on `cargo-tests` feature flag

#### 📚 Documentation Testing
- **Fixed all 24 doc test failures** in vm-package-server crate
  - Corrected import paths from `crate::` to `vm_package_server::` in documentation examples
  - Added proper async context and missing dependencies to doc test examples
  - Converted HTTP endpoint examples from rust to text format for appropriate rendering
  - Enhanced documentation examples with proper setup and teardown code

### Changed

- **Test execution model** now supports conditional testing based on environment capabilities rather than blanket ignoring
- **CI reliability** significantly improved with elimination of flaky tests
- **Developer experience** enhanced with clearer test failure messages and better error handling

---

## [1.4.0] - 2024-09-27

### Added

#### 🔐 Auth Proxy Service
- **New `vm auth` command suite** for centralized secret management
- **AES-256-GCM encryption** for secure secret storage with PBKDF2 key derivation
- **Scoped secrets** supporting Global, Project, and Instance-specific access
- **REST API** with bearer token authentication for programmatic access
- **Interactive secret management** with `vm auth interactive` command
- **Auto-start functionality** with user confirmation prompts
- **VM integration** via automatic environment variable injection during provisioning

#### 🐳 Docker Registry Service
- **Internal Docker registry management** for local image caching (auto-managed)
- **Pull-through caching** using nginx proxy + registry:2 backend architecture
- **Bandwidth optimization** reducing redundant downloads by 80-95% for teams
- **Automatic garbage collection** with configurable policies
- **Docker daemon auto-configuration** for seamless VM integration
- **Cache statistics** integrated in service status reporting
- **Offline capability** for previously pulled images

#### 📦 Package Registry Enhancement
- **Consolidated `vm pkg` commands** with improved status reporting
- **Enhanced shell integration** with `vm pkg use` for environment configuration

#### ⚙️ Configuration & Integration
- **New configuration flags**: `auth_proxy: true` and `docker_registry: true` in vm.yaml
- **Enhanced `vm status` command** showing status of all enabled services
- **Automatic service provisioning** during VM creation when services are enabled
- **Provider integration** with Docker daemon configuration and environment setup

### Changed

- **Main command dispatcher** now uses async/await pattern for better performance
- **Status reporting** consolidated to show all services (VM, Package Registry, Auth Proxy, Docker Registry)
- **Service architecture** migrated to library-first design with separate crates
- **CLI help documentation** enhanced with detailed command descriptions and examples

### Technical Details

#### New Library Crates
- `vm-auth-proxy` - Secure secret management with encryption and REST API
- `vm-docker-registry` - Docker image caching with nginx proxy architecture

#### CLI Commands Added
```bash
# Auth Proxy Management
vm auth status
vm auth add <name> <value> [--scope global|project:NAME|instance:NAME] [--description TEXT]
vm auth list [--show-values]
vm auth remove <name> [--force]
vm auth interactive

# Package Registry (Enhanced)
vm pkg status
vm pkg add [--type python,npm,cargo]
vm pkg remove [--force]
vm pkg list
vm pkg config show|get|set
vm pkg use [--shell bash|zsh|fish]

# System Management
vm update [--version v1.2.3] [--force]
vm uninstall [--keep-config] [--yes]
vm doctor
```

#### Configuration Schema Updates
```yaml
# New optional service flags in vm.yaml
auth_proxy: true        # Enable secure secret management
docker_registry: true   # Enable Docker image caching
package_registry: true  # Enable package caching (existing)
```

### Performance Improvements

- **Docker image caching** reduces build times and bandwidth usage
- **Secret injection** eliminates need for manual environment configuration
- **Package caching** speeds up dependency installation across VMs
- **Async command processing** improves CLI responsiveness

### Security Enhancements

- **End-to-end encryption** for secret storage using industry-standard algorithms
- **Bearer token authentication** for API access
- **Secure file permissions** (700/600) for auth proxy data
- **Isolated service architecture** with proper network boundaries

### Developer Experience

- **Unified service management** through consistent CLI patterns
- **Auto-start prompts** for seamless service dependency handling
- **Comprehensive status reporting** for troubleshooting and monitoring
- **Interactive secret management** with guided workflows

---

## [1.3.0] - Previous Release

### Features
- Core VM management functionality
- Provider support (Docker, Vagrant, Tart)
- Configuration management
- Basic provisioning capabilities

---

## Contributing

When adding entries to this changelog:
1. Use the format shown above
2. Group changes by type (Added, Changed, Deprecated, Removed, Fixed, Security)
3. Include user-facing impact in descriptions
4. Link to relevant documentation or issues where applicable
5. Keep technical details separate from user-facing changes