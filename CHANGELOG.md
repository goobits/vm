# Changelog

All notable changes to the VM tool will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Renamed yolo aliases to yo (yoclaude, yogemini, yocodex) and updated Gemini to use `--yolo`
- Made the optimized Vibe Dockerfile the default `Dockerfile.vibe`

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

- **Service Container Improvements** (from 4.5.x)
  - Container reuse for PostgreSQL and Redis services - avoids recreating existing containers
  - Fast exit in `vm create` when container already exists (unless --force)
  - Service container preservation during destroy/recreate workflows
  - Dynamic detection and explicit control of service containers
  - OpenAPI spec generation with utoipa

### Changed

- CLI command renames for better discoverability (see Added section)
- AI sync directories moved from ~/.config/vm/ to ~/.vm/ai-sync/ to avoid config pollution
- Service container orphan detection now warns instead of errors, allowing data preservation

### Fixed

- Snapshot @ prefix handling: `vm snapshot restore @name` and `vm snapshot delete @name` now correctly find global snapshots
- Pre-provisioned snapshots with different UIDs now work correctly
- .zshrc permission denied errors when building from snapshots
- Docker output streaming for long-running snapshot operations (no more silent hangs)
- Docker load output streaming during snapshot loading
- Zsh shell history permission errors when directory was created by root
- Missing aliases (yoclaude, yogemini, yocodex) in zsh shells

### Performance

- Skip redundant Ansible tasks when using snapshots (faster VM startup)
- Layered provisioning dramatically reduces time for customized VMs

## [4.4.3] - 2025-11-19

### Added

- **Pre-flight Validation for Orphaned Containers**: Detect orphaned service containers before VM creation
  - Checks for orphaned PostgreSQL containers from previous failed creation attempts
  - Warns users early with actionable cleanup guidance (`vm destroy --force`)
  - Prevents cryptic "container name already in use" errors during creation
  - Shows specific container names and manual cleanup commands

### Improved

- **Container Conflict Detection**: Enhanced error messages when container name conflicts occur
  - Detects "is already in use" or "Conflict" errors during `docker compose up`
  - Provides clear guidance with three options: auto-cleanup, inspect, or manual cleanup
  - Shows specific docker commands for debugging and manual intervention
  - Makes it easier to recover from partial creation failures

## [4.4.2] - 2025-11-19

### Added

- **Vibe Box Enhancements**: Added codex and tsx to global npm packages
  - Added `@openai/codex` CLI for OpenAI Codex integration
  - Added `tsx` for fast TypeScript execution
  - Added `yocodex` alias to Dockerfile.vibe bashrc

### Changed

- **Vibe Preset Cleanup**: Removed redundant aliases from vibe-dev preset
  - Aliases (yoclaude, yogemini, yocodex) are now only defined in Docker image
  - Eliminates duplication in generated vm.yaml files

### Fixed

- **Docker Service Port Validation**: Skip port validation for Docker service
  - Docker-in-Docker doesn't require port mapping (uses socket instead)
  - Fixes "Service 'docker' is enabled but has no port specified" error
  - Prevents validation errors when projects auto-detect Docker

- **VM Creation Panic**: Fixed Tokio runtime panic (exit code 134) when creating VMs
  - Replaced nested `Runtime::new()` with `tokio::task::block_in_place()` to use existing runtime
  - Fixes "Cannot start a runtime from within a runtime" error
  - VM creation now completes cleanly without panic

- **Terminal Configuration Not Applied**: Custom terminal config (emoji, username, git branch, theme, aliases) now works correctly
  - Root cause: Tokio panic was interrupting Ansible provisioning
  - Fix ensures Ansible completes successfully and applies full `.zshrc` configuration
  - Users now get custom prompt, NVM, shell history, syntax highlighting, and aliases

- **Snapshot Directory Mismatch**: Fixed `@` prefix handling for global snapshots
  - Regular `vm snapshot create @name` now correctly strips `@` and uses global scope
  - Global snapshots stored in `snapshots/global/name` (not `snapshots/default/@name`)
  - Consistent behavior between Dockerfile and regular snapshot creation paths

- **SSH Exit False Warnings**: Fixed "Session ended unexpectedly" on clean exit
  - Added `.unchecked()` to duct command to allow all exit codes through
  - Exit code 1 (last command failed) now treated as normal
  - Clean exits show "👋 Disconnected" instead of error warning

### Added

- **Snapshot List Enhancements**: Completed PROPOSED_CLI.md implementation
  - Added `--type base|project` filter flag to `vm snapshot list`
  - Shows TYPE column displaying "base" or "project" instead of project name
  - Displays explicit "Project: <name>" line for project snapshots
  - Clearer distinction between global and project-specific snapshots

## [4.4.1] - 2025-11-18

### Added

- **Comprehensive API & Web UI Documentation**
  - Complete quickstart guide with curl examples and UI walkthrough
  - Updated proposal files with implementation status and usage instructions
  - Step-by-step local development setup guide
  - Production deployment instructions

- **Two-Tier Preset System**: Clean separation between box and provision presets
  - Box presets: Pre-built snapshots (e.g., `@vibe-box`) for instant provisioning
  - Provision presets: Runtime package installation for customization
  - `PresetCategory` enum distinguishes preset types
  - `vm config preset --list`: Only shows provision presets (filters out box presets)
  - `vm init <preset>`: Accepts both box and provision presets
  - Box preset initialization creates minimal vm.yaml with box reference only
  - No duplicate package declarations when using box presets

### Fixed

- **Port Configuration**: Aligned with vm.yaml exposed port range (3120-3129)
  - API service now defaults to port 3121 (was 3000)
  - Web UI on port 3120 with automatic proxy to API on 3121
  - Both services accessible from host machine (bound to 0.0.0.0)

- **Preset Discovery**: Box presets properly filtered from config operations
  - Prevents confusion when listing available provision presets
  - Box presets remain available for `vm init` command

### Changed

- API and Web UI documentation consolidated in `proposals/` directory
- Vibe preset marked as `preset_category: box` for proper classification
- Preset content enhanced with networking, host_sync, and terminal fields

## [4.4.0] - 2025-11-17

### Added

- **VM Orchestrator Web UI (Phase 2)**: Complete web-based workspace management interface
  - Real-time workspace dashboard with status monitoring
  - Lifecycle controls (start/stop/restart) with visual feedback
  - Operations/activity tracking with auto-refresh (3-second polling)
  - Connection metadata display with one-click copy (SSH commands, container IDs)
  - Status badges (Connected, Disconnected, Provisioning) with color coding
  - "Open in Claude Code" quick action for running workspaces
  - Auto-refresh behavior (workspace list: 10s, active operations: 3s)

- **Snapshot Management UI**: Visual interface for workspace state preservation
  - Create snapshots with descriptive names
  - Restore snapshots with confirmation dialogs
  - Snapshot list with creation timestamps and sizes
  - Real-time operation status during snapshot create/restore
  - Snapshots stored in `/tmp/vm-snapshots/` with `.tar` format

- **CLI Configuration Autofix**: Interactive validation with automatic fixes
  - Validation errors now show specific details (e.g., "Service 'redis' is enabled but has no port specified")
  - Suggested fixes with available port assignments from configured range
  - Interactive prompt: "Apply suggested fixes?" with confirmation
  - Uses existing port range to intelligently assign available ports
  - Example: `services.redis.port → 3128: Assign available port to redis`

- **Comprehensive Documentation**
  - QUICKSTART.md guide covering all UI features and common workflows
  - Database migration documentation for vm-orchestrator
  - Integration test suite for vm-api and orchestrator

### Fixed

- **Critical Security**: Added owner authorization to operations routes
  - Operations now properly check workspace ownership before execution
  - Prevents unauthorized users from controlling other users' workspaces

- **CLI Preset Command**: Fixed directory handling for `vm config preset`
  - Now creates `vm.yaml` in current directory (not parent directories)
  - Validates presets exist before attempting initialization
  - Shows helpful "Preset not found" error with list of available presets
  - No longer searches parent directories (~/projects/ or ~/) for existing configs

- **Snapshot Operations**: Fixed actual disk state handling
  - Snapshots now properly capture and restore container filesystem state
  - Validate snapshot file exists before attempting restore
  - Proper error handling instead of unwrap() panics

- **UI Reactivity**: Fixed Svelte 5 reactivity for action buttons
  - Used object reassignment pattern for proper state updates
  - Prevents stale UI state during lifecycle operations
  - Action buttons now properly show loading states

- **Backend Operations**
  - Provisioner now updates operation status during workspace creation
  - Lifecycle operations (start/stop/restart) properly call vm-provider
  - Connection info regenerated after start/restart operations
  - Timestamp serialization fixed for consistent ISO 8601 format

### Changed

- UI source location consolidated to `/workspace/site/` (removed duplicate in `rust/site/`)
- Validation error messages now provide actionable guidance with field paths

## [4.3.0] - 2025-11-16

### Performance

- **Parallel snapshot operations** - Snapshot export/import operations now run in parallel for 2-5x faster multi-service snapshots
  - Image export/import parallelized using `buffer_unordered(3)`
  - Service snapshots parallelized using `buffer_unordered(4)`
  - Volume operations parallelized for concurrent processing

- **zstd compression for volumes** - Volume backups now use zstd instead of gzip for 3-5x faster compression/decompression
  - Parallel compression with `zstd -3 -T0`
  - Backward compatible with existing `.tar.gz` snapshots
  - Automatic format detection on restore

- **BuildX inline cache** - Docker builds now utilize BuildKit inline caching for dramatically faster incremental builds
  - First build: ~210s (unchanged)
  - Second build with cache: 30-90s (70% reduction)
  - Package-only changes: 15-30s (85% reduction)

- **Dockerfile layer optimization** - Reordered Dockerfile layers to minimize cache invalidation
  - Package installations moved to end of Dockerfile
  - Changing package lists only invalidates final layers (60% faster rebuilds)

- **APT cache optimization** - Added timestamp-based freshness check to skip unnecessary apt-get updates
  - Saves 30-60s on builds with recent cache
  - Reduces network traffic and improves build reliability

- **Snapshot export deduplication** - Snapshot exports now skip unchanged images based on digest comparison
  - 50% faster partial snapshot exports
  - Reduces disk I/O and storage requirements

- **Async container readiness detection** - Container startup polling now uses async/await with exponential backoff
  - Starts at 100ms, doubles each iteration, caps at 1s
  - Non-blocking operation improves responsiveness
  - Reduces CPU usage during container startup

- **Rayon parallel processing** - CPU-intensive snapshot operations now use parallel iterators
  - Directory size calculations parallelized with `par_bridge()`
  - Scales automatically with available CPU cores
  - 2-4x faster snapshot metadata generation

- **Async file I/O** - All snapshot file operations converted to non-blocking async I/O
  - Directory creation, file copying, and archiving now async
  - Prevents blocking on slow disk I/O
  - Better concurrency with parallel operations

- **String allocation reduction** - Hot paths optimized with `Cow<'_, str>` for zero-copy operations
  - Path expansion avoids allocations for ~70-80% of cases
  - Platform path conversion zero-copy on Unix systems
  - 5-10% reduction in memory allocations

- **Dynamic concurrency limits** - Parallel operations now adapt to available CPU count
  - Replaces hardcoded limits (3-4) with CPU-aware scaling (2-8)
  - Optimal resource utilization on multi-core systems
  - Protection against exhaustion on high-core-count machines

### Changed

- Snapshot volume archives now use `.tar.zst` format by default (`.tar.gz` still supported for restore)
- Parallel snapshot operations now scale dynamically with CPU count (2-8 concurrent operations)
- Docker build performance improvements bring overall performance score from 74/100 to ~93/100

## [4.2.0] - 2025-11-16

### Added

- **Podman Provider Support**: Rootless, daemonless container runtime as Docker alternative
  - Configure with `provider: podman` in vm.yaml
  - Full Docker CLI compatibility with enhanced security
  - Auto-detection of `podman compose` (built-in) vs `podman-compose` (standalone)
  - Complete feature parity with Docker provider

- **Snapshot Creation from Dockerfiles**: Simplified workflow for building custom base images
  - `vm snapshot create @name --from-dockerfile ./Dockerfile`
  - `--build-arg KEY=VALUE` flag for passing build arguments (repeatable)
  - Build context automatically determined from Dockerfile parent directory

- **Init Process for Containers**: Automatic zombie process reaping via tini
  - Prevents zombie process accumulation from test suites (Jest, Playwright, Vitest)
  - Proper signal forwarding (SIGTERM, SIGINT) to child processes
  - Enabled automatically via Docker's built-in tini

### Changed

- **BREAKING: Removed Vagrant Provider**
  - Migration: Update `provider: vagrant` to `provider: docker` or `provider: podman` in vm.yaml
  - Removed ~1,900 lines of code (12% reduction in vm-provider codebase)
  - Clearer focus: containers (Docker/Podman) + native macOS VMs (Tart)

### Performance

- **Snapshot Creation**: 97% faster (165s → 5s) by skipping UID/GID adjustment for pre-provisioned snapshots

### Fixed

- Preserve user customizations when applying presets (packages, versions, aliases)
- Suppress confusing warning when running `vm init` followed by `vm config preset`

## [4.1.1] - 2025-11-06

### Fixed

- **Vibe Preset Portability**: Restored declarative package lists for non-Docker scenarios
  - Works correctly for first-time Docker users (without @vibe-box snapshot)
  - Tart provider receives full package installation
  - Shell aliases (yoclaude, yogemini, yocodex) render properly

- **Snapshot Detection**: Fixed false positives using type-safe check
  - Prevents treating images like `company/dev-box:latest` as snapshots

## [4.1.0] - 2025-11-06

### Added

- **Snapshot-Based Provisioning Optimization**: Dramatic performance improvements
  - Smart package detection: Skip reinstalling packages already in snapshot
  - Intelligent UID/GID handling: Skip expensive file ownership operations
  - Performance: Fresh ~210s | With snapshot ~5-10s (95% faster)

- **Vibe Development Preset**: Pre-configured environment with @vibe-box snapshot
  - Includes Node.js 22, Python 3.14, Rust stable, Playwright, AI CLI tools
  - Pre-installed: tree, ripgrep, unzip, htop
  - Shell aliases for quick AI tool access

- **Snapshot References**: Use `@snapshot-name` syntax in presets via `vm.box: '@vibe-box'`

### Fixed

- Enhanced bash/zsh cross-compatibility
- Robust UID/GID mismatch handling
- Improved sudo installation and validation
- Port range reuse for re-initialized projects
- Default box image (ubuntu:jammy instead of ubuntu/jammy64)

## [4.0.0] - 2025-10-31

### Changed

- **BREAKING: Host Sync Configuration v2.0**: Consolidated under `host_sync` category
  - `copy_git_config` → `host_sync.git_config`
  - `ai_sync` → `host_sync.ai_tools`
  - `package_linking` → `host_sync.package_links`
  - `worktrees` → `host_sync.worktrees`
  - `development.ssh_agent_forwarding` → `host_sync.ssh_agent`
  - `development.mount_ssh_config` → `host_sync.ssh_config`
  - `development.sync_dotfiles` → `host_sync.dotfiles`

### Added

- **AI Tool Data Synchronization**: Multi-tool support for AI development
  - Configurable sync for Claude, Gemini, Codex, Cursor, and Aider
  - Boolean shorthand (`ai_tools: true`) or granular per-tool control
  - Automatic data directory creation and mounting

- Ansible support (auto-installed via pip for Python 3.13 compatibility)
- `--all-vms` flag for bulk VM file copying
- Progressive build output with real-time Docker build feedback

### Fixed

- VM destroy now respects container name argument
- Shell history persistence across container recreations
- Python 3.13 support

## [3.4.0] - 2025-10-30

### Added

- **Snapshot Import/Export System**: Share portable base images without Docker registry
  - `vm snapshot export @name [-o output.tar.gz]`
  - `vm snapshot import <file.tar.gz>`
  - Use imported snapshots as base images: `vm.box: @snapshot-name`
  - Snapshots stored in `~/.config/vm/snapshots/global/`

- **Enhanced VM Logs**: Powerful log viewing with follow mode and filtering
  - `-f/--follow` for live log streaming
  - `-n/--tail <lines>` for custom line count (default: 50)
  - `--service <name>` to filter by service (postgresql, redis, mongodb, mysql)

- **Bulk Database Backup**: `--all` flag for `vm db backup`
  - Automatically excludes system databases
  - Enhanced `vm db list` shows backup counts and sizes

- **Unified Box Configuration**: Consistent `vm.box` field across all providers
  - Smart detection: @snapshots, ./Dockerfiles, registry images
  - Provider-specific validation with helpful error messages

### Changed

- Default logging: `LOG_LEVEL=error` and `LOG_OUTPUT=file` (/tmp/vm.log) for quieter output
- Database backups use `GlobalConfig.backups.path` instead of hardcoded location

### Fixed

- Snapshot build conflicts via temporary project naming
- Database restore non-fatal during VM creation
- Network DNS resolution on external networks with aliases

## [3.3.0] - 2025-10-25

### Added

- **VM Snapshots**: Complete state preservation and restoration
  - `vm snapshot create/restore/list/delete <name>`
  - Captures containers, volumes, configurations, and git metadata
  - Project-specific storage at `~/.config/vm/snapshots`

## [3.2.0] - 2025-10-24

### Added

- **Dynamic Port Forwarding**: On-demand port tunneling
  - `vm port forward <host>:<container>` for ephemeral tunnels
  - `vm port list` and `vm port stop <port>`
  - Uses Docker relay containers for traffic forwarding

- **SSH Agent Forwarding**: Secure SSH key usage without copying private keys
  - Enable with `development.ssh_agent_forwarding: true`
  - Forwards SSH_AUTH_SOCK socket (read-only)
  - Optionally mounts ~/.ssh/config for host aliases

- **Selective Dotfiles Sync**: Mount configuration files from host
  - Configure with `development.sync_dotfiles` array
  - Examples: ~/.vimrc, ~/.config/nvim, ~/.tmux.conf
  - Read-only mounts with automatic tilde expansion

- Shell completion support (bash, zsh, fish, powershell)
- `vm wait` command to block until services are ready
- `vm ports` command to show all exposed port mappings
- Environment variable validation commands
- File watch fix for hot reload (inotify limits configured)
- Worktrees security enhancements (path validation, traversal protection)

## [3.1.1] - 2025-10-21

### Added

- Auto-build support for custom Dockerfiles
- PostgreSQL `PGPASSWORD` environment variable

### Fixed

- PostgreSQL service networking and database connection aliases
- Install script OpenSSL dependencies (`libssl-dev`)

## [3.1.0] - 2025-10-16

### Added

- **File Transfer Command**: `vm copy <source> <destination>` (similar to `docker cp`)
  - Upload: `vm copy ./local.txt /workspace/remote.txt`
  - Download: `vm copy my-vm:/remote/file.txt ./local.txt`
  - Auto-detect container: `vm copy ./file.txt /path/in/vm`
  - Works across all providers (Docker, Tart)

- **Unlimited CPU Support**: `cpus: unlimited` option matching memory
  - Container can use all available CPUs as needed
  - Docker's CFS scheduler manages distribution automatically

### Changed

- CPU configuration type: `Option<u32>` → `Option<CpuLimit>` enum (backward compatible)

## [3.0.0] - 2025-10-15

### Changed

- **BREAKING**: Renamed `vm provision` → `vm apply` for clarity and industry alignment
  - Migration: Replace all `vm provision` with `vm apply` in scripts

### Added

- **Docker Network Configuration**: Selective container-to-container communication
  - New `networking.networks: []` in vm.yaml
  - Networks automatically created during `vm create`
  - Containers on same network communicate using container names as hostnames

## [2.3.0] - 2025-10-12

### Added

- **Database Management Commands**: Comprehensive PostgreSQL lifecycle management
  - `vm db backup/restore/export/import/list/size/reset`
  - Auto-backup on `vm destroy` when destroying last VM using PostgreSQL
  - Configurable retention (default: keep 7 most recent)
  - Backups stored in `~/.vm/backups/postgres/`

- **Structured Logging System**: Multiple output formats
  - JSON output for machine parsing
  - Pretty-printed output with color coding
  - Configurable via `RUST_LOG` environment variable

- Automatic resource auto-adjustment for VM creation
- Global configuration auto-creation (`~/.vm/config.yaml`)
- Enhanced service integration (vm.yaml services + ServiceManager)

### Changed

- SSH start prompt defaults to "yes" for better UX

### Fixed

- Docker test skipping when Docker unavailable

## [2.2.0] - 2025-10-11

### Added

- **Shared Database Services**: Host-level database instances for all VMs
  - PostgreSQL, Redis, and MongoDB managed by ServiceManager
  - Automatic reference counting (start/stop based on VM usage)
  - Data persistence in `~/.vm/data/`
  - Environment variable injection (DATABASE_URL, REDIS_URL, MONGODB_URL)

- **Automatic Worktree Remounting**: SSH detects new Git worktrees
  - Interactive prompts when new worktrees detected
  - Safety checks prevent remounting during active SSH sessions

## [2.1.1] - 2025-10-10

### Added

- Tart provider feature parity (100% Docker parity)
  - SSH-based provisioning with framework detection
  - Automatic service installation
  - Enhanced status reports with batched SSH metrics
  - TempProvider trait implementation

## [2.1.0] - 2025-10-08

### Added

- **Git Worktrees Support**: Multi-branch development workflow
  - Project-scoped worktrees directory (`~/.vm/worktrees/project-{name}/`)
  - Automatic path repair using `git worktree repair`
  - Universal shell support (bash, zsh, sh, interactive, non-interactive)
  - Custom base path support with tilde expansion

### Fixed

- Git worktrees shell hook runs in ALL contexts (not just interactive zsh)
- Directory creation timing prevents Docker mount failures
- Windows platform detection with WSL2 validation

## [2.0.4] - 2025-09-30

### Fixed

- Ensure supervisord running before executing supervisorctl commands
- Container existence check before destroy operation
- BuildKit cache mount ownership
- PostgreSQL service provisioning reliability

### Performance

- Optimized Dockerfile with batch installs and BuildKit cache mounts

## [2.0.3] - 2025-09-26

### Added

- Co-located service ports with intelligent auto-allocation
- Auto-install preset plugins during VM installation
- Auto-configure package registry for all VMs

## [2.0.0] - 2024-09-28

### Added

- **Unified Platform Architecture**
  - `vm auth` - Centralized secrets management (AES-256-GCM encryption)
  - `vm pkg` - Package registry (npm, pip, cargo)
  - `vm create` - Smart project detection
  - Automatic service integration

- **Auth Proxy Service**: Centralized secrets with scoped access control
- **Docker Registry Service**: Local image caching (80-95% bandwidth savings)
- **Enhanced Package Registry**: Private package hosting

### Changed

- **BREAKING**: Simplified installation (single `./install.sh`)
- **BREAKING**: Removed `claude_sync`, `gemini_sync` (use `vm auth add`)
- **BREAKING**: CLI consolidated to single `vm` command

### Performance

- 60% faster VM creation
- 90% faster package installs
- 95% bandwidth savings for Docker images

## [1.4.0] - 2024-09-27

### Added

- Auth proxy service with encrypted secret storage
- Docker registry service with pull-through caching
- Enhanced `vm pkg` commands
- Configuration flags: `auth_proxy` and `docker_registry`

## [1.3.0] - Previous Release

### Features

- Core VM management functionality
- Provider support (Docker, Tart)
- Configuration and provisioning capabilities
