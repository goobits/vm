# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [4.6.0] - 2025-12-23

### Added

- **WebGL/WebGPU Support**: GPU-accelerated graphics in Docker containers
  - Mesa EGL/GL libraries (libegl1, libgl1-mesa-dri, libgles2-mesa) for SwiftShader WebGL backend
  - Vulkan support (libvulkan1, mesa-vulkan-drivers) for WebGPU
  - Firefox fallback for ARM64 WebGPU (native Chromium WebGPU unsupported on ARM64)
  - Architecture-aware Vulkan ICD paths (x86_64 and aarch64)
  - Virtual framebuffer (xvfb) with DISPLAY=:99 environment

- **Humane CLI Improvements**: More intuitive command names and workflows
  - `vm up` - Zero-to-code workflow (init → create → start → ssh in one command)
  - `vm clean` - Prune orphaned Docker resources (containers, images, volumes, networks)
  - `vm doctor --fix` - Automatic issue resolution (Docker daemon, socket permissions, SSH keys)
  - Renamed `vm auth` → `vm secrets` for clearer mental model
  - Renamed `vm port` → `vm tunnel` for simpler terminology
  - Renamed `vm pkg` → `vm registry` for clarity

- **Base Snapshot Management**: New `vm base` command for global snapshot management
  - `vm base list` - List all global base snapshots
  - `vm base create <name>` - Create @name snapshot
  - `vm base delete <name>` - Delete @name snapshot
  - `vm base restore <name>` - Restore from @name snapshot

- **Layered Provisioning**: Install project-specific packages on top of pre-built snapshots
  - Skip already-installed packages from base snapshot
  - `--refresh-packages` flag to force package reinstall for security updates
  - Performance: No custom packages ~0s, few packages ~10-30s, heavy customization ~1-2min

- **Auto-detect SSH Path**: SSH into nested directories automatically
  - Running `vm ssh` from packages/foo/src CDs into the same path inside VM
  - No need to manually specify --path flag

- **Claude Code Hardened Defaults**: Enhanced privacy and safety settings
  - Telemetry and error reporting disabled by default
  - Granular Bash command allowlist (replaces blanket allow)
  - Git guardrails prevent destructive operations (force push, reset, rebase)
  - Co-authored-by attribution for transparency

- **Web UI Enhancements**
  - Light/dark theme support with SSR-compatible zero-flash rendering
  - @goobits/docs-engine integration for documentation
  - @goobits/themes integration with ThemeProvider and ThemeToggle components

### Changed

- CLI command renames for better discoverability (see Added section)
- AI sync directories moved from ~/.config/vm/ to ~/.vm/ai-sync/ to avoid config pollution

### Fixed

- Snapshot @ prefix handling: `vm snapshot restore @name` and `vm snapshot delete @name` now correctly find global snapshots
- Pre-provisioned snapshots with different UIDs now work correctly
- .zshrc permission denied errors when building from snapshots
- Docker output streaming for long-running snapshot operations (no more silent hangs)
- Docker load output streaming during snapshot loading

### Performance

- Skip redundant Ansible tasks when using snapshots (faster VM startup)
- Layered provisioning dramatically reduces time for customized VMs

## [4.5.2] - 2025-11-21

### Added
- Centralized validation module in vm-core for server address validation (RFC 1123 hostnames, IPv4/IPv6, injection prevention)
- Fast exit in `vm create` when container already exists (unless --force)
- Container reuse for PostgreSQL and Redis services - avoids recreating existing containers
- Mismatch warnings when existing service containers have different port/image config

### Fixed
- Zsh shell history permission errors when directory was created by root
- Missing aliases (claudeyo, geminiyo, codexyo) in zsh shells - now embedded in templates

### Changed
- Security tests in vm-package-server now use shared validation module (DRY refactor)
- Improved preset loading in provisioner with proper PresetDetector fallback
- Better test isolation with preset cache clearing between tests

## [4.5.1] - 2025-10-09

### Fixed
- Container name borrowing in lifecycle operations

## [4.5.0] - 2025-01-20

### Added
- **Complete Podman support** - Podman provider now fully functional with executable parameter threaded through entire stack including docker-compose operations
- Modern web dashboard for VM API with theming support
- TestClient helper for API integration tests, reducing test boilerplate by ~40%

### Changed
- **Service container preservation** - Orphan detection now warns instead of errors when existing service containers (PostgreSQL, Redis, MongoDB) are found, allowing data preservation during destroy/recreate workflows
- Improved user messaging for orphaned containers with clearer guidance on data preservation vs fresh state

### Fixed
- Orphan detection no longer flags instance containers from other instances
- Unused variable warnings and clippy issues in provider code

## [4.4.3] - 2025-01-19

### Added
- Pre-flight validation for orphaned containers to prevent name conflicts during creation
- Enhanced conflict detection logic with actionable error messages

### Fixed
- Orphan detection functional gaps and conflict messaging
- Ansible 2.19 compatibility issues

## [4.4.2] - 2025-01-18

### Fixed
- Skip port validation for Docker service to prevent false positives
- Improved error reporting across provider layer

## [4.4.1] - 2025-01-17

### Added
- `--build-context` option to `vm snapshot create --from-dockerfile`

### Fixed
- Snapshot metadata saving for `--from-dockerfile` workflows
- Snapshot directory path resolution in VM creation
- Prevented silent overwrite when creating duplicate snapshots from Dockerfile
- Renamed snapshot 'default' directory to 'global' for consistency

### Performance
- Optimized Dockerfile.vibe with multi-stage builds and caching

## [4.4.0] - 2025-01-16

### Added
- **Two-tier preset system** - Separated box presets from provisioning presets for better modularity
- Comprehensive quickstart guide for vm-orchestrator UI
- Operations/activity visibility UI with real-time updates
- Enhanced connection metadata UI with copy buttons and status badges
- Lifecycle UI controls (start/stop/restart) with backend endpoints
- Snapshot UI and backend endpoints
- Operations tracking endpoints
- Integration test suite for vm-api and orchestrator
- Database migration documentation

### Changed
- Preset system now validates and distinguishes between box and provision categories
- Improved template handling in provisioner
- Connection info now displayed in workspace list UI

### Fixed
- ConfigOps::set signature and reduced nesting in validator
- Snapshot operations now copy/restore actual disk state
- Svelte reactivity for action buttons (use reassignment not mutation)
- Provisioner now updates operation status during workspace creation
- Lifecycle operations now properly call vm-provider start/stop/restart
- Snapshot file validation before restore
- Replaced unwrap() calls with proper error handling
- **Critical security fix** - Added owner authorization to operations routes
- Timestamp serialization in UI

### Performance
- Phase 2 performance optimizations (90→93/100 code quality score)
- BuildX cache implementation and 3 critical quick wins
- Priority 2 optimizations for snapshot operations

## [4.3.0] - 2025-01-10

### Added
- vm-api REST service with authentication
- vm-orchestrator with database-backed workspace management
- Web UI for workspace management (list/create/delete)
- Package registry trait integration

### Changed
- Complete service manager trait-based refactor
- Error handling improvements across the board
- Reduced function complexity in schema and formatting

### Fixed
- Hyphenated package names in PyPI registry
- General cleanup - split messages, reduce nesting

### Performance
- Phase 1 performance optimizations (90→93/100 code quality score)

## [4.2.0] - 2025-01-05

Earlier releases. See git history for details.

[Unreleased]: https://github.com/goobits/vm/compare/v4.6.0...HEAD
[4.6.0]: https://github.com/goobits/vm/compare/v4.5.2...v4.6.0
[4.5.2]: https://github.com/goobits/vm/compare/v4.5.1...v4.5.2
[4.5.1]: https://github.com/goobits/vm/compare/v4.5.0...v4.5.1
[4.5.0]: https://github.com/goobits/vm/compare/v4.4.3...v4.5.0
[4.4.3]: https://github.com/goobits/vm/compare/v4.4.2...v4.4.3
[4.4.2]: https://github.com/goobits/vm/compare/v4.4.1...v4.4.2
[4.4.1]: https://github.com/goobits/vm/compare/v4.4.0...v4.4.1
[4.4.0]: https://github.com/goobits/vm/compare/v4.3.0...v4.4.0
[4.3.0]: https://github.com/goobits/vm/compare/v4.2.0...v4.3.0
[4.2.0]: https://github.com/goobits/vm/releases/tag/v4.2.0
