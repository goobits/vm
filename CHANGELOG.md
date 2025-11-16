# Changelog

All notable changes to the VM tool will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [4.2.0] - 2025-11-16

### Added

- **Podman Provider Support**: Rootless, daemonless container runtime as Docker alternative
  - Usage: `provider: podman` in vm.yaml
  - Full Docker CLI compatibility with enhanced security
  - Auto-detection of compose command variants

- **Snapshot from Dockerfile**: Build snapshots directly from Dockerfiles
  - `vm snapshot create @name --from-dockerfile ./Dockerfile`
  - `--build-arg KEY=VALUE` for build arguments

- **Init Process**: Automatic zombie process reaping in containers via tini

### Changed

- **BREAKING: Removed Vagrant Provider**
  - Migration: Use `provider: docker` or `provider: podman`
  - Removed ~1,900 lines of code

### Performance

- **Snapshot Creation**: 97% faster (165s → 5s) by skipping UID/GID adjustment for pre-provisioned snapshots

### Fixed

- Preserve user customizations when applying presets
- Suppress warning when running `vm init` followed by `vm config preset`

## [4.1.1] - 2025-11-06

### Fixed

- Vibe preset portability for non-Docker scenarios
- Snapshot detection false positives (now uses type-safe check)

## [4.1.0] - 2025-11-06

### Added

- **Snapshot Optimization**: Smart package detection and UID/GID handling
  - Fresh builds: ~210s | With snapshot: ~5-10s (95% faster)
- **Vibe Preset**: Pre-configured dev environment with @vibe-box snapshot
  - Node.js 22, Python 3.14, Rust, Playwright, AI CLI tools
- **Snapshot References**: Use `@snapshot-name` syntax in presets

### Fixed

- Shell compatibility (bash/zsh cross-compatibility)
- UID/GID mismatch handling
- Sudo installation robustness
- Port range reuse for re-initialized projects

## [4.0.0] - 2025-10-31

### Changed

- **BREAKING: Host Sync v2.0**: Consolidated configuration under `host_sync`
  - `copy_git_config` → `host_sync.git_config`
  - `ai_sync` → `host_sync.ai_tools`
  - `package_linking` → `host_sync.package_links`
  - `worktrees` → `host_sync.worktrees`

### Added

- AI tool data sync (Claude, Gemini, Codex, Cursor, Aider)
- Ansible support (auto-installed via pip)
- `--all-vms` flag for bulk file copying
- Progressive build output with live feedback

### Fixed

- VM destroy respects container name argument
- Shell history persistence across recreations
- Python 3.13 support

## [3.4.0] - 2025-10-30

### Added

- **Snapshot Import/Export**: Share portable base images without registry
  - `vm snapshot export @name [-o file.tar.gz]`
  - `vm snapshot import <file.tar.gz>`
- Enhanced logs: `-f/--follow`, `-n/--tail`, `--service` filtering
- Bulk database backup: `--all` flag
- Unified `vm.box` field across all providers

### Changed

- Default `LOG_LEVEL=error` and `LOG_OUTPUT=file` for quieter output
- Database backups use `GlobalConfig.backups.path`

### Fixed

- Snapshot build conflicts (temporary project naming)
- Database restore non-fatal during VM creation
- Network DNS resolution on external networks

## [3.3.0] - 2025-10-25

### Added

- **VM Snapshots**: State preservation and restoration
  - `vm snapshot create/restore/list/delete`
  - Captures containers, volumes, configs, git metadata

## [3.2.0] - 2025-10-24

### Added

- **Dynamic Port Forwarding**: `vm port forward/list/stop`
- **SSH Agent Forwarding**: Secure SSH key usage without copying
- **Selective Dotfiles Sync**: Mount configs from host
- Shell completion support (bash, zsh, fish, powershell)
- `vm wait` command for service readiness
- `vm ports` command for port discovery
- Environment variable validation commands
- File watch fix (inotify limits for hot reload)

## [3.1.1] - 2025-10-21

### Added

- Auto-build support for custom Dockerfiles
- PostgreSQL `PGPASSWORD` environment variable

### Fixed

- PostgreSQL networking and aliases
- Install script OpenSSL dependencies

## [3.1.0] - 2025-10-16

### Added

- **File Transfer**: `vm copy` command (similar to `docker cp`)
- **Unlimited CPU**: `cpus: unlimited` option

## [3.0.0] - 2025-10-15

### Changed

- **BREAKING**: Renamed `vm provision` → `vm apply`

### Added

- Docker network configuration: `networking.networks` in vm.yaml

## [2.3.0] - 2025-10-12

### Added

- **Database Management**: `vm db` command suite
  - backup/restore/export/import/list/size/reset
  - Auto-backup on destroy, retention management
- Structured logging system (JSON/pretty output)
- Automatic resource auto-adjustment
- Global config auto-creation

### Changed

- SSH start prompt defaults to "yes"

## [2.2.0] - 2025-10-11

### Added

- **Shared Database Services**: Host-level PostgreSQL/Redis/MongoDB
  - Reference counting and automatic lifecycle
- Automatic worktree remounting on SSH

## [2.1.1] - 2025-10-10

### Added

- Tart provider feature parity (100% Docker parity)

## [2.1.0] - 2025-10-08

### Added

- **Git Worktrees Support**: Multi-branch development
  - Project-scoped directories, automatic path repair

### Fixed

- Shell hook runs in ALL contexts (not just interactive zsh)
- Windows platform detection with WSL2 validation

## [2.0.5] - 2025-10-02

## [2.0.4] - 2025-09-30

### Fixed

- Supervisord runtime checks
- Container existence check before destroy
- PostgreSQL provisioning reliability

### Performance

- Optimized Dockerfile with BuildKit cache mounts

## [2.0.3] - 2025-09-26

### Added

- Co-located service ports with intelligent auto-allocation
- Auto-install preset plugins
- Auto-configure package registry

## [2.0.2] - 2025-09-24

### Added

- Automatic service lifecycle management
- Enhanced plugin system

## [2.0.1] - 2025-09-23

### Added

- vm-messages system for centralized message management

## [2.0.0] - 2024-09-28

### Added

- **Unified Platform Architecture**
  - `vm auth` - Centralized secrets (AES-256-GCM)
  - `vm pkg` - Package registry (npm, pip, cargo)
  - `vm create` - Smart project detection
  - Automatic service integration

- **Auth Proxy Service**: Centralized secrets management
- **Docker Registry Service**: Local image caching (80-95% bandwidth savings)
- **Enhanced Package Registry**: Private package hosting

### Changed

- **BREAKING**: Simplified installation (single `./install.sh`)
- **BREAKING**: Removed `claude_sync`, `gemini_sync` (use `vm auth add`)
- **BREAKING**: CLI consolidated to single `vm` command

### Performance

- 60% faster VM creation, 90% faster package installs, 95% bandwidth savings

## [1.4.0] - 2024-09-27

### Added

- Auth proxy service with encrypted secret storage
- Docker registry service with pull-through caching
- Enhanced `vm pkg` commands
- Configuration flags: `auth_proxy` and `docker_registry`

## [1.3.0] - Previous Release

### Features

- Core VM management
- Provider support (Docker, Vagrant, Tart)
- Configuration and provisioning
