# Changelog

All notable changes to the VM tool will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [4.1.1] - 2025-11-06

### Fixed

- **Vibe Preset Portability**: Restored declarative package lists and aliases for non-Docker scenarios
  - Preset now works correctly for first-time Docker users (without @vibe-base snapshot)
  - Vagrant and Tart providers receive full package installation
  - Shell aliases (claudeyolo, geminiyolo, codexyolo) now render properly
  - Docker with @vibe-base still uses fast path via smart package checks (5-10s unchanged)
  - Falls back gracefully to full installation when snapshot unavailable

- **Snapshot Detection Safety**: Fixed false positives in pre-provisioned snapshot detection
  - Now uses explicit BoxConfig::Snapshot check instead of string matching
  - Prevents treating unrelated images like `company/dev-box:latest` as snapshots
  - Eliminates risk of skipping base system setup for non-snapshot images
  - More accurate and safer detection logic

### Technical

- Changed snapshot detection from heuristic (`contains("-box")`) to type-safe check
- Restored full preset configuration for provider portability
- Maintained Docker performance optimization via smart package existence checks

## [4.1.0] - 2025-11-06

### Added

- **Snapshot-Based Provisioning Optimization**: Dramatic performance improvements for snapshot-based VM creation
  - Intelligent package detection: Skip reinstalling packages already present in snapshot
  - Smart UID/GID handling: Skip expensive file ownership operations for pre-provisioned snapshots
  - Incremental package installation for APT, NPM, and PIP packages
  - **Performance impact:**
    - Fresh projects: ~210s (unchanged)
    - With snapshot (UID matches): ~5-10s (95% faster) 🚀
    - With snapshot (UID differs): ~10-15s (93% faster)
  - Enhanced snapshot detection: Supports `-base` and `-box` suffix patterns

- **Vibe Development Preset**: Pre-configured development environment with @vibe-base snapshot
  - Includes Node.js 22, Python 3.14, Rust stable, Playwright, and AI CLI tools
  - Pre-installed packages: tree, ripgrep, unzip, htop
  - Pre-configured AI tools: Claude Code, Gemini CLI, OpenAI Codex
  - Shell aliases for quick AI tool access (claudeyolo, geminiyolo, codexyolo)
  - Minimal preset configuration leverages snapshot for instant startup

- **Snapshot Box Reference Support**: Use `@snapshot-name` syntax in presets
  - Presets can now reference global snapshots via `vm.box: '@vibe-base'`
  - Supports unquoted @ symbols in snapshot box references
  - Automatic detection and optimization for snapshot-based images

### Fixed

- **Shell Compatibility**: Enhanced bash/zsh cross-compatibility
  - Added early return guard in .bashrc for non-bash shells
  - Bash-specific commands now properly guarded with `$BASH_VERSION` checks
  - Prevents errors when zsh sources bash configuration files

- **UID/GID Mismatch Handling**: Robust user ID synchronization between host and container
  - Graceful handling of GID conflicts (uses existing group instead of failing)
  - Conditional file ownership fixes (skipped for pre-provisioned snapshots)
  - Force root user during setup to avoid "user currently used by process 1" errors
  - Improved permission handling for shell history and config directories

- **Sudo Installation**: More robust sudo setup and validation
  - Automatic sudo package installation if missing from base image
  - Optional visudo validation (handles cases where visudo isn't installed yet)
  - Correct sudoers file permissions (0440)
  - Automatic /etc/sudoers.d directory creation

- **Provisioning Robustness**: Enhanced provisioning reliability
  - Install packages before running Ansible configuration
  - Strategic guards to prevent redundant expensive operations
  - Improved locale generation and healthcheck
  - Better snapshot permission handling

- **Port Range Management**: Fixed port allocation for re-initialized projects
  - Reuse existing port ranges when re-initializing projects
  - Prevents port conflicts and unnecessary port reassignments

- **Configuration Defaults**: Fixed incorrect default box image
  - Changed default from `ubuntu/jammy64` (Vagrant) to `ubuntu:jammy` (Docker)
  - Updated schema documentation to clarify Docker vs Vagrant box formats

### Changed

- **Vibe Preset Structure**: Streamlined to minimal snapshot-referencing configuration
  - Removed redundant package lists (now provided by @vibe-base snapshot)
  - Removed aliases (baked into snapshot's .bashrc)
  - Retained only runtime behavior: vm.box reference and host_sync configuration
  - Clearer separation between baked-in (snapshot) and runtime (preset) concerns

## [4.0.0] - 2025-10-31

### Changed

- **BREAKING: Host Sync Configuration Consolidation (v2.0)**: All host-to-VM synchronization features consolidated under unified `host_sync` category
  - **Migration required**: Update your `vm.yaml` configuration files to v2.0 syntax
  - **Field mappings:**
    - `copy_git_config` → `host_sync.git_config`
    - `development.ssh_agent_forwarding` → `host_sync.ssh_agent`
    - `development.mount_ssh_config` → `host_sync.ssh_config`
    - `development.sync_dotfiles` → `host_sync.dotfiles`
    - `ai_sync` → `host_sync.ai_tools`
    - `package_linking` → `host_sync.package_links`
    - `worktrees` → `host_sync.worktrees`
  - **Benefits:**
    - Cleaner, more organized configuration structure
    - All host synchronization settings in one logical place
    - Better discoverability and documentation
  - **Migration example:**
    ```yaml
    # v1.x (OLD - no longer supported)
    copy_git_config: true
    ai_sync: true
    package_linking:
      npm: true
    worktrees:
      enabled: true

    # v2.0 (NEW - required)
    host_sync:
      git_config: true
      ai_tools: true
      package_links:
        npm: true
      worktrees:
        enabled: true
    ```

### Added

- **AI Tool Data Synchronization**: Enhanced multi-tool support for AI development environments
  - Configurable sync for Claude, Gemini, Codex, Cursor, and Aider
  - Boolean shorthand (`ai_tools: true`) or granular control per tool
  - Automatic data directory creation and mounting
  - Part of unified `host_sync` configuration

- **Ansible Support**: First-class Ansible support for infrastructure automation
  - Auto-installed via pip for Python 3.13 compatibility
  - macOS compatibility improvements
  - Integrated into development environment

- **Copy Command Enhancement**: Bulk VM file copying
  - New `--all-vms` flag to copy files across all VMs simultaneously
  - Useful for distributing configuration updates

- **Progressive Build Output**: Real-time Docker build feedback
  - Live progress display when building custom Dockerfiles
  - Wrapper Dockerfile progress indication
  - Container startup progress feedback
  - Bypasses log level for critical build output

- **Development Environment Improvements**:
  - Unconditional `~/.local/bin` in PATH for user binaries
  - git-lfs (Large File Storage) support
  - Auto-detection of latest stable Python version
  - Version output for all auto-detected languages

### Fixed

- **VM Destroy**: Now correctly respects container name argument instead of always using default
- **Shell History Persistence**: Ensures shell history survives across `vm ssh` sessions
- **Snapshot Build Improvements**:
  - Auto-force-recreate for snapshot builds from Dockerfile
  - Always destroy stale containers when forcing recreate
  - Skip service registration for snapshot builds
  - Resolve container name conflicts
- **User Permissions**: Fixed permission handling in wrapper Dockerfiles
- **Python 3.13 Support**: Updated Dockerfile Python installation for deadsnakes PPA

## [3.4.0] - 2025-10-30

### Added

- **Snapshot Import/Export System**: Share portable base images across machines without Docker registry
  - New `vm create --save-as @name --from-dockerfile <path>` to build and save global snapshots
  - New `vm snapshot export @name [-o output.tar.gz]` to export snapshots as compressed archives
  - New `vm snapshot import <file.tar.gz>` to import snapshots from archive files
  - Support for `vm.box: @snapshot-name` to use imported snapshots as base images (instant startup)
  - Snapshots stored in `~/.config/vm/snapshots/global/`
  - **Benefits:**
    - Build complex Dockerfiles once (10-15 min), share instantly across team
    - 5-10 second VM creation vs 10-15 minute rebuilds
    - Team collaboration without Docker registry
    - Air-gapped environment support
  - Includes example `Dockerfile.vibe` with Node.js LTS, Python 3.13, Rust stable, Playwright

- **Enhanced VM Logs Command**: Powerful log viewing with follow mode and filtering (Docker provider only)
  - New `-f/--follow` flag for live log streaming
  - New `-n/--tail <lines>` flag to show custom number of lines (default: 50)
  - New `--service <name>` flag to filter logs by service (postgresql, redis, mongodb, mysql)
  - Example: `vm logs -f --tail 100 --service postgresql`

- **Bulk Database Backup**: Streamlined database management
  - New `--all` flag for `vm db backup` to backup all databases at once
  - Automatically excludes system databases (postgres, template0, template1)
  - Shows success/failure count after bulk operations
  - Enhanced `vm db list` now displays backup counts and sizes per database

- **Unified Box Configuration**: Consistent `vm.box` field across all providers
  - Smart string detection: `@snapshots`, `./Dockerfiles`, registry images
  - Support for Docker build context and arguments in Dockerfile specs
  - Cross-platform path handling (Windows + Unix)
  - Provider-specific validation with helpful error messages
  - Full backwards compatibility with `vm.box_name` (deprecated but supported)

### Changed

- **Logging System**: Reduced console noise for cleaner output
  - Default `LOG_LEVEL` changed from "info" to "error"
  - Default `LOG_OUTPUT` changed from "console" to "file" (/tmp/vm.log)
  - Users can enable verbose logging with: `LOG_LEVEL=info LOG_OUTPUT=console`
  - Eliminates structured tracing logs, docker version output, and container listings

- **Database Backup System**: Configuration consolidation
  - Uses `GlobalConfig.backups.path` instead of hardcoded `~/.vm/backups`
  - Respects user-configured backup location with tilde expansion
  - Consolidated retention on `GlobalConfig.backups.keep_count` (default: 5)

- **VM Init Command**: Improved consistency
  - Deduplicated initialization logic to ensure consistent config generation
  - Both `vm init` and `vm config preset X` now produce identical configs
  - Reduced code from 108 lines to 20 lines by delegating to vm-config library

### Fixed

- **Snapshot Build Conflicts**: Temporary project naming prevents collisions
  - When using `--from-dockerfile` with `--save-as`, tool now uses temporary project name
  - Format: `{snapshot-name}-build` to avoid conflicts with existing vm.yaml
  - Automatic cleanup of temporary build container after snapshot is saved
  - Respects explicit user instructions over auto-detected configurations

- **Database Restore Failures**: Non-fatal backup restoration
  - Database restore now non-fatal during VM creation (won't block VM startup)
  - Default backup pattern matches project name to prevent unrelated backup conflicts
  - New `restore_backup` config flag to disable restoration
  - Clear warning messages when restore fails

- **Network DNS Resolution**: Service discovery on external networks
  - Added network aliases for containers on external networks (e.g., "jules")
  - Containers now reachable by name instead of requiring IP addresses
  - Main dev container gets alias: `{project_name}-dev`
  - Service containers get proper aliases (e.g., `{project_name}-postgres`, `postgres`)

- **Vibe Preset**: Fixed claude_sync and gemini_sync configuration
  - Added missing `claude_sync: true` and `gemini_sync: true` flags
  - Enables proper AI tool installation in vibe preset

### Removed

- **Legacy Ansible Backup System**: Cleaned up orphaned backup/restore logic
  - Removed Docker-incompatible backup restoration code
  - Users should use modern `vm db backup`/`restore` commands
  - Auto-backup still available with `backup_on_destroy: true`

## [3.3.0] - 2025-10-25

### Added

- **VM Snapshots**: Complete state preservation and restoration system
  - New `vm snapshot create <name>` command to capture entire VM state
  - New `vm snapshot restore <name>` command to restore from snapshot
  - New `vm snapshot list` command to show available snapshots
  - New `vm snapshot delete <name>` command to remove snapshots
  - Captures containers, volumes, configurations, and git metadata
  - Docker integration with commit, save/load, and volume backup
  - Project-specific storage at `~/.config/vm/snapshots`
  - **Use cases:**
    - Save development environment as reusable template
    - Share pre-configured environments across team
    - Quick environment switching between projects
    - Backup before major configuration changes
  - Snapshots are designed as permanent reusable templates (like Docker images)
  - Single command restoration of complete environments

### Changed

- **Examples Workflow**: Migrated from file-based examples to snapshot-based templates
  - Replaced 11 legacy example files with snapshot migration documentation
  - Enhanced examples/README.md with snapshot workflow guide
  - Updated user guide with comprehensive snapshot documentation

### Removed

- **Legacy Example Files**: Removed obsolete example configurations
  - Removed `examples/base-images/QUICKSTART.md`
  - Removed `examples/base-images/README.md`
  - Removed `examples/base-images/build.sh`
  - Removed `examples/base-images/minimal-node.dockerfile`
  - Removed `examples/base-images/supercool.dockerfile`
  - Removed `examples/configurations/full-stack.yaml`
  - Removed `examples/configurations/minimal.yaml`
  - Removed `examples/nextjs-app/vm.yaml`
  - Removed `examples/services/mongodb.yaml`
  - Removed `examples/services/postgresql.yaml`
  - Removed `examples/services/redis.yaml`

## [3.2.0] - 2025-10-24

### Added

- **Dynamic Port Forwarding**: On-demand port tunneling for debugging and testing
  - New `vm port forward <host>:<container>` command for ephemeral port tunnels
  - `vm port list` to show active port forwarding tunnels
  - `vm port stop <port>` to stop specific tunnels or `--all` for all tunnels
  - Uses Docker relay containers with `alpine/socat` for traffic forwarding
  - Network namespace sharing with target container for seamless routing
  - State tracked in `~/.config/vm/tunnels/active.json`
  - Automatic cleanup of dead containers
  - **Use cases:**
    - Debugging: `vm port forward 9229:9229` for Node.js inspector
    - Multi-VM testing: Different host ports → same container ports
    - Temporary services: On-demand database access without permanent configuration
  - No port conflicts between VMs - each tunnel is independently managed

- **SSH Agent Forwarding**: Secure SSH key usage inside VMs without copying private keys
  - Enable with `development.ssh_agent_forwarding: true` in vm.yaml
  - Forwards `SSH_AUTH_SOCK` socket into container (read-only)
  - Optionally mounts `~/.ssh/config` for host aliases (read-only)
  - Enables git operations with GitHub, GitLab, and SSH-based services
  - Private keys never exposed to VMs - maximum security
  - Opt-in per project configuration

- **Selective Dotfiles Sync**: Mount configuration files from host without copying
  - Configure with `development.sync_dotfiles` array in vm.yaml
  - Mount specific files or directories (e.g., `~/.vimrc`, `~/.config/nvim`, `~/.tmux.conf`)
  - Automatic tilde (`~`) expansion for home directory
  - Read-only mounts to prevent accidental changes on host
  - Path validation with warnings for non-existent files
  - Preserves directory structure in container
  - **Use cases:**
    - Vim/Neovim users: Consistent editor configuration
    - Tmux + Shell users: Persistent terminal environment
    - Full-stack developers: Sync tool configs (.npmrc, .pypirc, .gitconfig)

- **Shell Completion Support**: Tab completion for all vm commands
  - New `vm completion <shell>` command for bash, zsh, fish, and powershell
  - Enables autocomplete for commands, subcommands, and flags
  - Installation: `vm completion bash > /usr/share/bash-completion/completions/vm`

- **Service Wait Command**: Block until services are ready
  - New `vm wait` command with configurable timeout (default: 60s)
  - Wait for specific service: `vm wait --service postgresql`
  - Wait for all services: `vm wait`
  - Useful for CI/CD pipelines: `vm start && vm wait && npm run migrate`

- **Port Discovery Command**: Show all exposed port mappings
  - New `vm ports` command displays host→container port mappings
  - Shows service health status for each port
  - Helps identify which ports are in use

- **Environment Variable Validation**: Catch misconfigurations early
  - New `vm env validate` to check .env against template
  - `vm env diff` to show differences between environments
  - `vm env list` to display all variables (with sensitive value masking)
  - Prevents deployment with missing required environment variables

- **Shell History Persistence**: History survives container recreations
  - Docker volume mount for persistent bash/zsh history
  - History preserved across `vm destroy && vm create` cycles

- **File Watch Fix**: Hot reload for webpack, vite, nodemon
  - Configured `fs.inotify` limits in docker-compose
  - Sets `max_user_watches=524288` and `max_user_instances=256`
  - Fixes hot reload issues with modern development tools

- **Worktrees Security Enhancements**:
  - Path validation to prevent `VM_WORKTREES` override to dangerous system directories
  - Path traversal protection with realpath validation
  - Enhanced worktree name validation with path traversal detection

- **Discovery UX Improvements**:
  - First-time user tips on `vm ssh` explaining vm-worktree commands
  - Helpful error messages and troubleshooting guidance

## [3.1.1] - 2025-10-21

### Added
- **Supercool Base Image Enhancements**
  - Added `jspcd` npm package for JavaScript/TypeScript project analysis
  - Added cargo tools: `cargo-nextest`, `cargo-watch`, `cargo-udeps`
  - Added `mold` linker for faster Rust builds
  - Added Makefile targets: `make watch`, `make dev`, `make udeps`

- **Auto-Build Support for Custom Base Images**
  - Automatic detection and building of custom Dockerfiles
  - Checks if image exists locally before building
  - 5-15 minute build time on first run, instant on subsequent uses

- **PostgreSQL Improvements**
  - Added `PGPASSWORD` environment variable for passwordless `psql` connections

### Fixed
- PostgreSQL service networking and Docker database connection aliases
- Install script now includes OpenSSL dependencies (`libssl-dev`)

## [3.1.0] - 2025-10-16

### Added
- **File Transfer Command**: New `vm copy` command for simple file transfer between host and VMs
  - Syntax: `vm copy <source> <destination>` (similar to `docker cp`)
  - Supports both upload to VM and download from VM
  - Auto-detects container name when in project directory
  - Works across all providers (Docker, Vagrant, Tart)
  - Examples:
    ```bash
    vm copy ./local.txt /workspace/remote.txt      # Upload
    vm copy my-vm:/remote/file.txt ./local.txt    # Download
    vm copy ./file.txt /path/in/vm                # Auto-detect container
    ```
  - Docker provider uses native `docker cp` for fast transfers
  - Vagrant/Tart providers use SSH-based transfer with cat/redirect

- **Unlimited CPU Support**: CPU configuration now supports "unlimited" option matching memory
  - Configure with `cpus: unlimited` in vm.yaml for no CPU limits
  - Dynamic CPU allocation - container can use all available CPUs as needed
  - Docker's CFS scheduler manages CPU distribution automatically
  - Multiple containers can share CPUs efficiently
  - Example configuration:
    ```yaml
    vm:
      cpus: unlimited  # No CPU limit (or specify number like: 4)
      memory: unlimited  # No memory limit (or specify MB like: 8192)
    ```
  - Validation skips CPU checks when set to unlimited
  - Status commands display "unlimited" for unlimited CPU/memory

### Changed
- **CPU Configuration Type**: `vm.cpus` field changed from `Option<u32>` to `Option<CpuLimit>` enum
  - Maintains backward compatibility - integer values still work
  - Serializes as integer or "unlimited" string
  - Custom deserializer accepts both formats seamlessly

### Fixed
- **YAML Formatting**: Corrected indentation for `networking.networks` field in configuration
- **Type Safety**: Fixed compilation errors in resource auto-detection code

## [3.0.0] - 2025-10-15

### Breaking Changes
- **Renamed `vm provision` to `vm apply`** for better clarity and industry alignment
  - The `provision` command has been renamed to `apply` to match conventions from Terraform and Kubernetes
  - This better reflects the command's purpose: applying configuration changes to running containers
  - **Migration**: Replace all instances of `vm provision` with `vm apply` in scripts and workflows
  - All user-facing messages and documentation updated accordingly

### Added
- **Docker Network Configuration**: Selective container-to-container communication
  - New `networking.networks: []` configuration in `vm.yaml`
  - Containers can join one or more Docker networks for inter-container communication
  - Networks are automatically created during `vm create` if they don't exist
  - Network name validation (1-64 characters, alphanumeric with `-_` allowed)
  - **Example use case**: Share a `backend-services` network between multiple projects
  - Containers on the same network can communicate using container names as hostnames
  - Updated docker-compose template to support custom network configuration

### Changed
- **Command naming**: `provision` → `apply` throughout codebase
  - CLI command updated in vm/src/cli/mod.rs
  - Handler function renamed from `handle_provision` to `handle_apply`
  - All messages updated in vm-messages crate
  - Documentation updated across all markdown files

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

### Fixed
- **Worktree Tests**: Updated tests for new dynamic detection approach

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

## [2.1.0] - 2025-10-08

### Added
- **Git Worktrees Support**: First-class support for git worktrees allowing developers to work on multiple branches simultaneously
  - Project-scoped worktrees directory (`~/.vm/worktrees/project-{name}/`)
  - Automatic path repair for seamless host/container workflow using `git worktree repair`
  - Universal shell support (bash, zsh, sh, interactive, non-interactive, docker exec, VS Code, CI/CD)
  - Opt-in configuration via global (`~/.vm/config.yaml`) or per-project (`vm.yaml`) settings
  - Custom base path support with tilde expansion
  - Platform detection with WSL2 support (Linux, macOS, WSL2 supported; Windows native blocked with clear error)

### Fixed
- Git worktrees: Shell hook now runs in ALL shell contexts (previously only interactive zsh)
- Git worktrees: Directory creation timing fixed to prevent Docker mount failures
- Git worktrees: Added Windows platform detection with WSL2 validation
- Supervisor: Removed redundant restart causing permission errors

## [2.0.5] - 2025-10-02

## [2.0.4] - 2025-09-30

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

### Fixed
- Docker fixes for service port allocation and conflict resolution
- Correct Cargo package index path structure
- Add auto-restart capability to critical services
- Remove workspace directory creation from Dockerfile to prevent permission issues

## [2.0.2] - 2025-09-24

### Added
- Automatic service lifecycle management
- Enhanced plugin system capabilities

## [2.0.1] - 2025-09-23

### Added
- Initial vm-messages system foundation for centralized message management
- Message templates for better internationalization (i18n) support

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

### Security Enhancements

- **End-to-end encryption** for secret storage using industry-standard algorithms
- **Bearer token authentication** for API access
- **Secure file permissions** (700/600) for auth proxy data
- **Isolated service architecture** with proper network boundaries

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