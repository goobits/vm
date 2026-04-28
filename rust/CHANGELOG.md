# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [4.8.4] - 2026-04-28

### Fixed

- **Tart Codex Provisioning**: Tart guests now avoid Linux-only mount assumptions and repair Codex auth/config state for macOS shells
- **Tart Provisioner Structure**: AI tooling, shell config, and temporary mount handling now live in smaller focused provisioning modules
- **Version Sync Safety**: `version-sync` now ignores VM config schema versions and reports check results directly

## [4.8.3] - 2026-04-26

### Changed

- **Provider Switching CLI**: Use direct provider commands for the main workflow
  - `vm create tart` or `vm create docker` creates/configures a provider-specific environment
  - `vm start tart` or `vm start docker` runs a one-off provider session from the same project
  - `vm ssh`, `vm stop`, `vm status`, `vm logs`, and `vm destroy` accept `docker`, `podman`, or `tart` as project provider selectors
  - `vm exec --provider <provider>` and `vm copy --provider <provider>` cover commands whose positional arguments are already command/path data
  - `vm use tart` or `vm use docker` saves the project default and matching profile when available
  - `vm destroy tart` or `vm destroy docker` destroys the current project on a specific provider
  - Docs now reserve `--provider` for provider-scoped subcommands such as `vm base` and `vm fleet`

### Fixed

- **Docker Shell History Permissions**: Docker shell entry now repairs the persistent history volume before launching `zsh`
- **Destroy Provider Resolution**: `vm destroy` now destroys the provider that actually owns the current project instance instead of mixing Docker detection with Tart deletion
- **Destroy Confirmation Clarity**: Destroy prompts now show the provider and use provider-specific resource labels for Docker containers and Tart VMs
- **Tart Destroy Reliability**: Tart destroy now force-stops a running VM before deleting it
- **Docker AI CLI Provisioning**: Claude, Gemini, and Codex install failures now fail at the install step with useful output, and Claude is linked into `~/.local/bin` when its installer chooses another user-local path

## [4.8.2] - 2026-04-24

### Fixed

- **Docker And Tart Shell Hardening**: Interactive and non-interactive shell behavior is now more consistent across providers
  - Docker `vm ssh` and `vm exec` now use the target user environment more reliably
  - Tart `vm ssh` and `vm exec` now handle shell startup and workspace paths more safely
  - Shared shell rendering now avoids more cross-provider prompt and path drift

- **Shared Shell Robustness**: The canonical `zsh` template is now safer for mixed Docker and Tart workflows
  - Workspace paths and project alias commands are rendered through shell-safe metadata
  - macOS Tart shell defaults no longer assume Linux-only aliases or permissions paths
  - Claude permissions now include the default macOS Tart workspace path

## [4.8.1] - 2026-04-23

### Changed

- **Tart Defaults Now Match The Product Model**: `vibe-tart` now targets macOS guests by default instead of Ubuntu-on-Tart
  - macOS Tart guests now use the correct workspace mount behavior and writable workspace path
  - The public preset surface is now `vibe` for Docker and `vibe-tart` for macOS Tart
  - `vm base build vibe --provider tart` now builds the standard macOS Tart base by default

- **Shared Shell Experience Across Docker And Tart**: Tart now uses the same canonical `zsh` shell template as Docker
  - Tart macOS guests now get the same prompt, theme, aliases, and shell defaults as Docker-backed `vibe`
  - Tart shell overrides were reduced to runtime environment exports only, removing duplicate PATH, NVM, and alias setup
  - Default prompt emoji is now inferred by provider when `terminal.emoji` is omitted: Docker `🐳`, Tart macOS `🍎`, Tart Linux `🐧`

### Fixed

- **Tart Provider Reliability**: The Tart create/start flow now matches the provider-first workflow more closely
  - `vibe-tart` now points at the correct macOS Tart base and SSH user by default
  - Applying the `vibe-tart` preset preserves provider profiles and Tart-specific settings
  - Reapplying `vibe-tart` now removes the old preset-derived rocket emoji so inferred provider defaults can show through
  - Prebaked Tart vibe bases no longer reinstall baseline AI CLIs during normal provisioning
  - Tart failures now include better host log context for guest startup and provisioning issues
  - Exiting a Tart shell no longer reports a false session error

- **Tart Base Build Robustness**: Building the default macOS Tart base is less brittle
  - macOS Tart guest provisioning now uses the correct virtiofs mount flow instead of Linux-only mount commands
  - Canonical Tart shell rendering is covered by a regression test

- **Documentation Cleanup**: Main docs now match the current Docker, Podman, and Tart workflow
  - Removed stale Vagrant references from current provider docs
  - Updated command references from `vm auth` and `vm pkg` to `vm secrets` and `vm registry`
  - Clarified Tart as the native macOS VM path and Docker as the default fast path

## [4.8.0] - 2026-04-23

### Added

- **Provider-First Runtime Selection**: Choose Docker or Tart directly from the main CLI flow
  - One-off Tart usage from the same project
  - Persisting the project default provider
  - Provider selection now applies the matching provider profile automatically when available

- **Unified Vibe Base Workflows**: Docker and Tart now use the same base-environment command model
  - `vm base build vibe --provider docker`
  - `vm base build vibe --provider tart`
  - `vm base validate vibe` for shared provider validation

### Changed

- **Faster Docker Vibe Startup**: Snapshot-backed Docker boxes now avoid more unnecessary rebuild and provisioning work
  - Reuses derived images more effectively for unchanged vibe inputs
  - Removes per-create Claude update behavior
  - Moves more host-specific setup out of image layers and into runtime configuration

- **Stronger Tart Vibe Parity**: Tart now behaves much closer to Docker for the shared `vibe` workflow
  - Tart-native vibe base build path with shared tooling expectations
  - Better host sync for AI configs, dotfiles, and SSH config
  - Cleaner Python provisioning via `pipx` and project virtual environments

## [4.7.1] - 2026-04-02

### Fixed

- Dockerfile snapshot operations now default to global snapshots, avoiding project-scoped mismatches
- VM startup handles stale Git worktrees more cleanly and surfaces underlying I/O errors
- Docker provider errors include clearer diagnostics when listing containers and related resources fails

### Changed

- Reduced duplicate `vm status` output and unnecessary compose regeneration during status checks

## [4.7.0] - 2026-01-29

### Added

- **Fleet Commands**: Manage multiple VMs at once
  - `vm fleet start/stop/status` for bulk operations
  - Default targets running instances

- **Tart Provider Improvements**: Better macOS VM support
  - Tart profile in vm.yaml configuration
  - Fixed exec command syntax for current Tart CLI

- **Destroy UX Improvements**
  - Interactive arrow-key selector replaces ABC text prompts
  - `--remove-services` flag for explicit service cleanup

### Fixed

- **Container Conflicts**: `vm start` now handles orphaned containers with `--force-recreate`
- Tart VM name resolution (uses project name without `-dev` suffix)
- Tart status reporting returns proper errors when VM not found
- Sudo home directory resolution and shell escaping
- Startup verification reliability

### Changed

- Simplified presets to: `vibe`, `vibe-tart`, and `base` (removed language/framework presets)
- Removed deprecated CLI commands: `env`, `profile`, `migration`
- Removed deprecated `box_name` config field

### Performance

- Regex compilation optimized with `OnceLock` across codebase:
  - DockerProgressParser, VersionSync, config validation
  - Cargo/pip package detection and parsing
- Package listing uses `spawn_blocking` to avoid blocking event loop
- HashSet lookup for pip package matching
- Manual parsing replaces regex in config validation

## [4.6.0] - 2025-12-23

### Added

- **WebGL/WebGPU Support**: GPU-accelerated graphics in Docker containers
  - Mesa EGL/GL libraries (libegl1, libgl1-mesa-dri, libgles2-mesa) for SwiftShader WebGL backend
  - Vulkan support (libvulkan1, mesa-vulkan-drivers) for WebGPU
  - Firefox fallback for ARM64 WebGPU (native Chromium WebGPU unsupported on ARM64)
  - Architecture-aware Vulkan ICD paths (x86_64 and aarch64)
  - Virtual framebuffer (xvfb) with DISPLAY=:99 environment

- **Humane CLI Improvements**: More intuitive command names and workflows
  - `vm start` - Zero-to-code workflow (init → create → start → ssh in one command)
  - `vm clean` - Prune orphaned Docker resources (containers, images, volumes, networks)
  - `vm doctor --fix` - Automatic issue resolution (Docker daemon, socket permissions, SSH keys)
  - Renamed `vm auth` → `vm secrets` for clearer mental model
  - Renamed `vm port` → `vm tunnel` for simpler terminology
  - `vm registry` is now the canonical package registry command

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
- Missing aliases (yoclaude, yogemini, yocodex) in zsh shells - now embedded in templates

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

[Unreleased]: https://github.com/goobits/vm/compare/v4.8.1...HEAD
[4.8.1]: https://github.com/goobits/vm/compare/v4.8.0...v4.8.1
[4.8.0]: https://github.com/goobits/vm/compare/v4.7.1...v4.8.0
[4.7.1]: https://github.com/goobits/vm/compare/v4.7.0...v4.7.1
[4.7.0]: https://github.com/goobits/vm/compare/v4.6.0...v4.7.0
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
