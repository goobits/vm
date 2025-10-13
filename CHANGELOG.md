# Changelog

All notable changes to the VM tool will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.3.0] - 2025-10-12

### Added
- **Database Management Commands**: Comprehensive PostgreSQL database lifecycle management
  - **New `vm db` command suite**:
    - `vm db backup <db_name> [--name <backup_name>]` - Create compressed database dumps
    - `vm db restore <backup_name> <db_name>` - Restore from backup
    - `vm db export <db_name> <file>` - Export to plain SQL file
    - `vm db import <db_name> <file>` - Import from SQL file
    - `vm db list` - List all databases
    - `vm db size` - Show database disk usage
    - `vm db reset <db_name> [--force]` - Drop and recreate database
  - **Automated backup management**:
    - Auto-backup on `vm destroy` when destroying last VM using PostgreSQL
    - Configurable backup retention (default: keep 7 most recent)
    - Automatic cleanup of old backups
    - Database seeding from SQL file on `vm create`
  - **Enhanced destroy UX**:
    - Warns when destroying last VM that uses PostgreSQL
    - Shows database name, location, and size information
    - Suggests creating backup before destruction
  - **Configuration options**:
    - Global: `services.postgresql.auto_backup` and `backup_retention`
    - Project: `services.postgresql.backup_on_destroy` and `seed_file`
  - Backups stored in `~/.vm/backups/postgres/` with timestamp naming

- **Structured Logging System**: Comprehensive logging infrastructure with multiple output formats
  - JSON output for machine parsing and log aggregation systems
  - Pretty-printed output with color coding for human readability
  - Configurable via `RUST_LOG` environment variable
  - Integration with tracing ecosystem for performance monitoring
  - New `vm-logging` crate for centralized logging configuration

- **Automatic Resource Auto-Adjustment**: Smart resource allocation for VM creation
  - Analyzes available host resources (CPU, memory, disk)
  - Automatically adjusts VM configuration to prevent resource conflicts
  - Provides recommendations for optimal VM performance
  - Reduces failed VM creation due to insufficient resources
  - Less conservative memory auto-adjustment for better utilization

- **Global Configuration Auto-Creation**: Improved first-run experience
  - Automatically creates `~/.vm/config.yaml` if it doesn't exist
  - Eliminates manual configuration file creation step
  - Pre-populates with sensible defaults
  - Enhances onboarding experience for new users

- **Enhanced Service Integration**: vm.yaml services now integrate with ServiceManager
  - Project-specific service configurations respected by global ServiceManager
  - Seamless coordination between project and global service settings
  - Improved service lifecycle management

- **Port Forwarding Tests**: New integration tests for port mapping functionality
  - Tests for single and multiple port mappings
  - Port conflict detection validation
  - UDP and TCP protocol support verification

### Changed
- **SSH Start Prompt**: Default answer changed to "yes" for better UX
  - Pressing Enter now accepts the default (start VM)
  - Reduces friction in common workflow
  - Users can still explicitly decline with "n" or "no"

- **Onboarding Experience**: Multiple improvements for new users
  - Better error messages and troubleshooting guidance
  - Enhanced quick start documentation
  - Improved development workflow documentation

### Fixed
- **Docker Test Skipping**: Tests gracefully skip when Docker is unavailable
  - Prevents test failures in environments without Docker
  - CI/CD friendly with clear skip messages
  - Improved test reliability across different environments

### Removed
- **Platform Module**: Removed obsolete platform module code
  - Cleaned up unused abstractions
  - Simplified codebase architecture

### Documentation
- Added project audit report and bug tracking documentation
- Consolidated testing documentation, kept only action plan
- Removed completed database persistence planning document
- Split bug reports into focused proposals
- Reordered proposals by priority and dependencies

### Technical Improvements
- **Database environment variables integration test**: Comprehensive test coverage
  - Verifies DATABASE_URL, REDIS_URL, MONGODB_URL injection
  - Tests correct host addressing (172.17.0.1 on Linux, host.docker.internal on macOS/Windows)
  - Validates service auto-start/stop behavior
  - Prevents regression of missing environment variables issue
- Added port forwarding integration tests with TestFixture pattern
- Enhanced test coverage for service configuration
- Improved code organization and maintainability
- All tests passing with improved reliability
- New crate: `vm-logging` for structured logging support

## [2.2.0] - 2025-10-11

### Added
- **Shared Database Services**: Host-level database instances accessible to all VMs
  - PostgreSQL, Redis, and MongoDB services managed by ServiceManager
  - Automatic reference counting (start when first VM needs it, stop when last VM destroyed)
  - Data persistence across VM lifecycles in `~/.vm/data/`
  - Environment variable injection (`DATABASE_URL`, `REDIS_URL`, `MONGODB_URL`)
  - Opt-in configuration via global `~/.vm/config.yaml`
  - Comprehensive integration tests for service lifecycle
  - See [Shared Services User Guide](docs/user-guide/shared-services.md)

- **Automatic Worktree Remounting**: SSH automatically detects new Git worktrees and offers to refresh container mounts
  - Interactive prompts when new worktrees are detected (auto-accepts "yes" on empty input)
  - Safety checks prevent remounting when multiple SSH sessions are active
  - Session tracking via `~/.vm/state/{project}.json` for active SSH connection counting
  - Provider interface extended with `get_container_mounts()` for current mount inspection
  - Helper function `detect_worktrees()` scans git metadata for worktree paths
  - Integration tests in `vm/tests/ssh_refresh.rs`

- **Developer Onboarding Improvements**
  - Enhanced quick start documentation
  - Improved error messages and troubleshooting guides
  - Better development workflow documentation

### Changed
- **Dependency Updates**: Pinned all workspace dependencies to specific patch versions for reproducible builds
  - Updated 28 dependencies including serde (1.0.228), clap (4.5.11), anyhow (1.0.82), tokio (1.47)
  - Full list in commit 1fbf036

- **Code Modernization**
  - Applied clippy's `uninlined_format_args` suggestions across codebase
  - Use inline format args: `format!("{key}")` instead of `format!("{}", key)`

### Fixed
- **Worktree Tests**: Updated tests for new dynamic detection approach
- **Documentation**: Synchronized all documentation with worktree remounting feature

### Documentation
- Enhanced shared services guide with troubleshooting and per-project isolation
- Updated Development Guide (docs/DEVELOPMENT.md) with ServiceManager architecture details
- Updated README with shared database services feature
- Synchronized documentation with automatic worktree remounting feature

### Technical Improvements
- Added `VmState` struct for tracking active SSH sessions per project
- Enhanced `handle_ssh()` with worktree detection and mount comparison logic
- Implemented `worktrees_match()` helper for efficient mount verification
- Extended `GlobalConfig` with PostgreSQL, Redis, and MongoDB settings
- Added service lifecycle management to `ServiceManager` (start, stop, health checks)
- All worktree-related tests passing (7/7)
- Service lifecycle integration tests expanded and passing

## [2.1.1]

### Added
- **Tart Provider Feature Parity**: Achieved 100% Docker parity for macOS-native VMs
  - SSH-based provisioning with framework detection (Node.js, Python, Ruby, Rust, Go)
  - Automatic service installation (PostgreSQL, Redis, MongoDB)
  - Custom provision script support (`provision.sh`)
  - Enhanced status reports with batched SSH metrics collection (CPU, memory, disk, uptime, services)
  - ProviderContext support for dynamic CPU/memory configuration updates without VM recreation
  - TempProvider trait implementation for temporary VM workflow support
  - Force kill implementation using `tart stop --force` for hung VMs
  - SSH path handling fix to correctly navigate to specified directories

### Fixed
- Missing `init_config_file` export in vm-config library causing compilation errors

### Changed
- Tart provider rating upgraded from ⭐⭐⭐ (30% advanced features) to ⭐⭐⭐⭐⭐ (100% complete)

### Removed
- Completed PROPOSAL_TART_PROVIDER_PARITY.md (all features implemented)

### Technical Improvements
- Added `rust/vm-provider/src/tart/provisioner.rs` (255 lines) for comprehensive provisioning
- Added `rust/vm-provider/src/tart/scripts/collect_metrics.sh` (48 lines) for metrics collection
- Enhanced `rust/vm-provider/src/tart/provider.rs` (+274 lines) with all missing Provider trait methods
- All workspace tests passing (77 vm-config + 37 vm-provider tests)

## [2.1.0] - 2025-10-08

### Added
- **Git Worktrees Support**: First-class support for git worktrees allowing developers to work on multiple branches simultaneously
  - Project-scoped worktrees directory (`~/.vm/worktrees/project-{name}/`)
  - Automatic path repair for seamless host/container workflow using `git worktree repair`
  - Universal shell support (bash, zsh, sh, interactive, non-interactive, docker exec, VS Code, CI/CD)
  - Opt-in configuration via global (`~/.vm/config.yaml`) or per-project (`vm.yaml`) settings
  - Custom base path support with tilde expansion
  - Platform detection with WSL2 support (Linux, macOS, WSL2 supported; Windows native blocked with clear error)
- Automatic version bumping with `make build` command
- Comprehensive git worktrees proposal documentation
- Improved provider error handling

### Fixed
- Git worktrees: Shell hook now runs in ALL shell contexts (previously only interactive zsh)
- Git worktrees: Directory creation timing fixed to prevent Docker mount failures
- Git worktrees: Added Windows platform detection with WSL2 validation
- Supervisor: Removed redundant restart causing permission errors
- Clippy warnings: Fixed `field_reassign_with_default` in test code (8 instances)

### Changed
- Git worktrees: `get_worktrees_host_path()` visibility changed to `pub` for lifecycle access
- Git worktrees: Applied modern Rust idioms (`is_some_and` instead of `map_or`)
- Vagrant provider: Thread-unsafe `env::set_var()` replaced with per-command environment variables
- Tart provider: Deduplicated VM creation logic (130 lines reduced, 62% less duplication)
- Extracted `extract_project_name()` helper to common module for reuse across providers
- Vagrant/Tart: Implemented `create_with_context()` trait methods for Provider interface

### Removed
- Tart provider: Dead code `create_instance()` method (36 lines)
- Obsolete error improvement proposal document

### Documentation
- **Complete documentation cleanup (Phases 1-3)**: Comprehensive overhaul of all project documentation
  - Phase 1 (Critical): Fixed invalid CLI commands, corrected configuration examples, overhauled all plugin READMEs
  - Phase 2 (Major): Added Git worktrees documentation, synchronized CHANGELOG, standardized preset naming
  - Phase 3 (Standardization): Created plugin README template, refactored all 10 plugin READMEs with consistent structure
- Created standard `PLUGIN_README_TEMPLATE.md` for consistent plugin documentation
- Updated all plugin READMEs with: detailed package descriptions, enabled services, configuration examples, use cases, troubleshooting
- Fixed invalid CLI examples in user guides (removed non-existent flags like `--raw`, `--container-id`)
- Corrected vm-package-server documentation (removed 130+ lines of non-existent commands)
- Added comprehensive Git worktrees usage guide and troubleshooting
- Updated test coverage statistics in testing.md (305 total tests)

### Technical Improvements
- 7 new git worktrees tests (all passing)
- 490 total tests passing (100% pass rate)
- Zero regressions detected
- Auto-formatted code per project standards
- Vagrant/Tart providers now compile with `--all-features` flag
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