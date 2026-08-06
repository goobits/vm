# Changelog

<!-- CHANGELOG audit cutoff: 2026-08-05. commit 32c128a2 on main. -->

## [Unreleased]

### 🌟 Highlights

- 🪟 Default shell and exec connections now resolve the configured environment and start it when stopped.
- ☁️ Container storage policies add durable scoped volumes, bounded tmpfs, resource limits, and log rotation.
- ☁️ Fingerprinted dependency and Playwright bootstrap skips unchanged work and exposes runtime storage evidence.
- ☁️ Lifecycle cleanup and removal preserve managed data and complete configured database backups before deletion.
- 🚀 macOS Tart environments use Sequoia and support Docker through Colima with QEMU software emulation.
- 🚀 `vm create` builds a missing standard Tart vibe base automatically at the configured storage location.
- 📦 Redacted `vm config render` output previews the exact generated provider configuration.

### ✨ Added

- 🪟 `vm restart` provides the same humane environment targeting as start and stop operations.
- ☁️ Container storage configuration supports scoped named volumes, bounded tmpfs mounts, PID limits, graceful-stop timing, and log rotation.
- 🚀 Targeted status reports include writable-layer size, volume and tmpfs usage, memory and PID peaks, mounts, logging, and lifecycle settings.
- 🚀 Explicit pnpm-store pruning is available through `vm doctor --prune-pnpm-store`.
- 📦 `vm config render [--instance <name>]` previews redacted generated configuration without contacting the provider.

### 🔧 Changed

- 🪟 Environment listing is project-aware by default, with `vm list --all` providing the global inventory.
- 🪟 Profile and target selection consistently prefer explicit names, configured defaults, canonical environments, sole matches, then an interactive choice.
- 🪟 Shell, SSH, and exec commands start an existing stopped environment before connecting without creating a missing one.
- 🪟 CLI output and actionable error hints now follow one consistent stdout and stderr contract.
- 🚀 macOS Tart environments use the Sequoia base and run Docker through Colima with QEMU software emulation instead of unsupported nested virtualization.

### 🐛 Fixed

- 🗂️ Cross-platform home and log discovery now resolves host package paths, Tart logs, and uninstall targets without Unix-only fallbacks.
- ☁️ AI tool state directories remain writable by the environment user across Docker and Tart provisioning.
- ☁️ Environment removal completes configured database backups before destruction, and cleanup removes only VM-managed disposable volumes.
- ☁️ Snapshot exports publish atomically, preventing partial archives from appearing at the requested destination after interruption or disk exhaustion.
- ☁️ Generated Compose, container configuration, and port-registry files use atomic writes to avoid partial state after failures.
- ☁️ Temporary-environment state locks time out with recovery guidance instead of hanging indefinitely.
- 🌐 Package-registry commands no longer panic inside the async runtime.
- 🌐 Docker image pulls retry transient transport failures while permanent authentication and image errors fail immediately.
- 🌐 Single-port ranges and explicit create-time port mappings now validate correctly.
- 🚀 Tart stop operations are idempotent, and resolved stopped guests start reliably before shell connections.
- 🚀 Docker client and service provisioning no longer depends on Python APT bindings and avoids reinstalling tools that are already present.
- 🚀 Missing standard Tart vibe bases are built automatically when `vm create` needs them, including when `tart.storage_path` selects another disk.
- 📦 Empty Docker inspection results now report an error instead of returning a misleading default status.

### ⚡ Performance

- ☁️ Fingerprinted bootstrap skips locked dependency and browser installation when the relevant inputs have not changed.
- 🚀 Host package discovery performs one Cargo, npm, or pipx inventory lookup per package manager instead of one subprocess per requested package.

### 🔒 Security

- 👤 Authentication proxy checks use constant-time token comparison, bounded request bodies, atomic secret persistence, and rejection audit logs without exposing tokens.
- ☁️ Snapshot imports reject absolute paths, traversal entries, symlinks, and hardlinks.
- ☁️ Temporary host mounts resolve symlinks before validation and reject paths that escape into protected host locations.
- 🌐 Docker-published application and service ports honor `vm.port_binding`, defaulting to loopback.
- 🚀 Tart package provisioning shell-quotes configured package names to prevent guest command injection.
- 📦 Copied environment configuration is readable only by its container owner.

### 🏠 Internal

- 📦 CLI command ownership, provider lifecycle contracts, instance-state queries, and target resolution were consolidated behind smaller explicit boundaries.
- 📦 The former `vm-cli` message helper moved into `vm-core`, unused glob exports were removed, and shared host-home resolution replaced provider duplication.
- 🧪 Workspace dependencies, Rust 1.90 compatibility, CI execution, formatting gates, and output-contract regression coverage were refreshed.
- 📚 CLI, architecture, testing, publishing, plugin, and provider guidance now use one canonical VM 5.x documentation set.

## 5.0.0

### 🔧 Changed

- 🪟 Humane v5 commands center daily work on `run`, `list`, `shell`, `exec`, `logs`, `copy`, lifecycle, state, configuration, plugin, and system operations.
- 🚀 Lower-level registry and base-image workflows live under `vm system`.
- 📦 Database, fleet, and secret workflows remain plugin-backed top-level commands.
- 📚 Documentation describes the v5 command model.

### 🐛 Fixed

- ☁️ Environment removal preserves explicitly saved snapshots.
