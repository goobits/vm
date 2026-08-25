# Changelog

<!-- CHANGELOG audit cutoff: 2026-08-21. commit 856cd08f on main. -->

## [Unreleased]

### 🌟 Highlights

- 🪟 Default shell connections create the configured environment when missing, while shell and exec connections start it when stopped.
- ☁️ Container storage policies add durable scoped volumes, bounded tmpfs, resource limits, and log rotation.
- ☁️ Provider-neutral tool caches and fingerprinted bootstrap preserve downloads while skipping unchanged dependency and Playwright work.
- ☁️ Lifecycle cleanup and removal preserve managed data and complete configured database backups before deletion.
- 🚀 `vibe-tart` defaults to a Linux guest with Docker Engine while retaining an explicit macOS/Colima fallback.
- 🚀 Environment creation builds a missing standard Tart vibe base automatically at the configured storage location.
- 📦 Redacted `vm config render` output previews the exact generated provider configuration.
- 📦 One-command private tool releases auto-register attested producers and activate globally enrolled fleets without recreating environments or volumes.

### ✨ Added

- 🪟 `vm restart` provides the same humane environment targeting as start and stop operations.
- ☁️ Container storage configuration supports scoped named volumes, bounded tmpfs mounts, PID limits, graceful-stop timing, and log rotation.
- 🚀 Targeted status reports include writable-layer size, volume and tmpfs usage, memory and PID peaks, mounts, logging, and lifecycle settings.
- 🚀 Explicit pnpm-store pruning is available through `vm doctor --prune-pnpm-store`.
- 📦 `vm config render [--instance <name>]` previews redacted generated configuration without contacting the provider.
- 📦 Successful local `vm packages register <path>` calls remember exact read-only canonical workspaces, enabling repository-bound private releases without placing projects in managed source shelves.
- 📦 Private package publication automatically prepares tested review branches for every drifted registered consumer.
- 📦 Binary tools use a durable, no-egress build stage whose unprivileged source commands cannot access release, publish, Git, or queue credentials.
- 📦 Docker binary builders place their queue credential beneath a root-only mount boundary instead of relying on Compose secret modes that Docker Desktop does not enforce consistently.
- 📦 `vm packages checkout <source>` creates or resumes a guest-owned checkout from the managed guest's signed consumer identity.
- 📦 Source-only package checkouts skip inapplicable consumer validation, while cancellation restores guest dependency state before closing its retryable durable checkout.
- 📦 Credential-isolated package services translate registered GitHub SSH origins to token-authenticated HTTPS without receiving host SSH keys.
- 📦 Large first-time package checkouts use a bounded source-preparation timeout and clean interrupted clone state before retry.
- 📦 Same-version source builds refresh Docker package edges by immutable image identity, delivering updated managed clients without recreating environments.
- 📦 Catalog reconciliation treats GitHub HTTPS and SSH forms as one canonical repository instead of degrading after a local transport change.
- 📦 Explicit cross-project Docker targets load their owning project configuration before managed-tool and package-edge reconciliation.
- 📦 `vm tools update [<tool>...]` filters each running managed Docker environment's configured tools by default, supports repeated exact `--to` targeting across providers, and excludes stopped environments unless explicitly included.
- 📦 `vm tools enable <tool>...` persists global tool enrollment and immediately activates selected tools across running managed Docker environments; future environments inherit the same baseline.
- ☁️ `vm tools update codex claude antigravity` updates VM-owned vendor CLIs from declarative official-installer definitions with staging, validation, rollback, and no environment rebuild.
- 📦 Package gateway upstreams now follow Docker DNS changes after appliance service replacement instead of retaining stale container addresses.
- 📦 `vm packages release` queues one durable activation for each binary or collection release, resumes after interruption, and defers stopped environments until startup.
- 📦 Attested canonical tool workspaces register their exact signed source automatically during release without granting repository choice to guests.

### 🔧 Changed

- ☁️ Vibe base builds replace deprecated Gemini CLI with Antigravity and use one shared native installer contract for Antigravity, Claude Code, and Codex; `agent-skills` remains managed by `vm tools`.
- ☁️ Vibe presets no longer attach projects to the `spacebase` network unless explicitly configured.
- 🪟 Environment listing is project-aware by default, with `vm list --all` providing the global inventory.
- 🪟 Profile and target selection consistently prefer explicit names, configured defaults, canonical environments, sole matches, then an interactive choice.
- 🪟 Shell and SSH create a missing environment from `vm.yaml`; shell, SSH, and exec start an existing stopped environment before connecting.
- 🪟 CLI output and actionable error hints now follow one consistent stdout and stderr contract.
- 🚀 Linux-first `vibe-tart` routing uses the versioned Tart base and Docker Engine directly; macOS/Colima remains an explicit fallback.
- 🚀 macOS Tart environments use the Sequoia base and run Docker through Colima with QEMU software emulation instead of unsupported nested virtualization.
- 📦 ⚠️ Bare `vm packages release` infers a managed checkout or registered canonical workspace from the current directory; the retired host `packages work` launcher and public checkout-ID lifecycle commands are removed.
- 📦 ⚠️ `vm tools update` positional values are tool filters only; exact environment selection now uses repeated `--to <environment>` options.
- 📦 ⚠️ New tool manifests use schema 1; legacy collection manifests remain readable during migration, while retired package-work keys and the tool-specific `vm tools update --fleet` flag are rejected.
- 📦 Managed source shelves and exact canonical project roots now have separate policies: only shelves are recursively discovered or quarantined, while canonical roots remain read-only and require a repository-bound v2 guest capability.
- 📦 Package infrastructure uses one Docker-or-Podman control plane shared by container and Linux Tart environments instead of maintaining a second appliance inside Tart.
- 📦 Source-installed package appliances fingerprint server and job inputs independently, skip unchanged image builds, and use the faster optimized source-install profile for changed local images.
- 🚀 macOS builds always include the Tart provider through target-specific dependency wiring, including packaged releases and source installs.

### 🐛 Fixed

- ☁️ Worktree creation refuses pre-existing non-worktree directories, rejects sibling-prefix escapes, and preserves failed partial worktrees for explicit inspection instead of recursively deleting them.
- 🪟 Source installation fails closed when the official Rust installer checksum is unavailable or malformed instead of executing a size-only-verified download.
- 🪟 Source installation atomically copies `vm` into the stable user binary directory and keeps reusable Cargo artifacts in the platform cache, so temporary-build cleanup cannot break the installed CLI.
- ☁️ Vendor-tool migration safely adopts broken symlinks inside a declared legacy installer scope while continuing to refuse unrelated launchers.
- 🪟 Existing `vm.box` configurations remain readable as a deprecated alias for canonical `vm.image`.
- 📦 Deterministic package review permits only bounded, comment-only `.env.example` templates while continuing to reject environment values, credentials, private keys, and other sensitive paths.
- 📦 Managed tool checkouts remain available until fleet activation succeeds, and release waits long enough for the activation worker's bounded per-target retries, making its retry guidance executable on larger fleets.
- 📦 Restored managed checkouts recreate their durable submission ref before integration, allowing compacted and first-release checkouts to proceed without another agent commit.
- 📦 The first managed checkout of an unpublished source can release its canonical committed tree directly, without requiring a meaningless empty commit.
- 📦 Existing managed Docker environments created before or during ownership-label rollout remain discoverable from their exact project identity and workspace configuration, preventing shell connections and tool activation from entering a create/recreate or unresolved-owner path.
- 📦 Existing package appliances migrate their persisted `runtime` and `review_image` metadata to the canonical state shape before normal commands run, so upgrades do not block `vm ssh` or `vm packages up`.
- 📦 macOS source builds now compile and start the launchd tool-activation worker correctly.
- 📦 Legacy managed collections remain discoverable during schema migration, and quarantine repair preserves equivalent GitHub SSH origins instead of requiring HTTPS.
- 📦 The built-in `agent-skills` source and equivalent catalog migrations now retain SSH as their canonical Git transport.

- 🗂️ Cross-platform home and log discovery now resolves host package paths, Tart logs, and uninstall targets without Unix-only fallbacks.
- ☁️ AI tool state directories remain writable by the environment user across Docker and Tart provisioning.
- ☁️ Environment removal completes configured database backups before destruction, and cleanup removes only VM-managed disposable volumes.
- ☁️ Snapshot exports publish atomically, preventing partial archives from appearing at the requested destination after interruption or disk exhaustion.
- ☁️ Forced snapshot creation and import stage replacements before swapping them into place, preserving the previous snapshot when preparation fails.
- ☁️ PostgreSQL restores validate dumps and restore into a staging database before replacing the current database.
- ☁️ Generated Compose, container configuration, and port-registry files use atomic writes to avoid partial state after failures.
- ☁️ Temporary-environment state locks time out with recovery guidance instead of hanging indefinitely.
- 🌐 Package-registry commands no longer panic inside the async runtime.
- 🌐 Docker package-appliance startup reuses present immutable image overrides, enabling local release acceptance without an unnecessary registry pull.
- 🌐 Rust workspace Docker builds exclude platform-specific target trees and dependency directories from their build context.
- 🌐 Package-service image builds use an explicit in-image Cargo target directory instead of inheriting the host workspace output path.
- 🌐 Package-job images disable Node's preinstalled Corepack shims before installing pinned pnpm and Yarn versions.
- 🌐 Package appliances attach only their credential-free gateway to a host-facing controller bridge, keeping registry and workflow storage internal while making the configured gateway port reachable.
- 🌐 Package appliances retain explicit image overrides across same-version restarts while allowing a newer controller release to select its matching images.
- 🌐 Source-installed Docker package appliances automatically build matching infrastructure images when unreleased image tags are unavailable; released installs remain pull-only.
- 🌐 Source-installed package appliances recheck local service and job images through Docker's content-addressed build cache, preventing non-controller source edits from leaving stale infrastructure behind.
- 🌐 Source checkout discovery follows the installed `vm` symlink, and a networkless initialization step repairs non-root package-volume ownership before services start.
- 🌐 Managed collections now publish through a credential-isolated ephemeral job, and `vm tools update` bootstraps the built-in `agent-skills` source and initial immutable release.
- 🌐 The persistent private release service receives its publish secret as a read-only file instead of an unresolved Compose mount.
- 🌐 Package workflow retries resume durable checkout, review, integration, release, and rollout state without duplicating work.
- 🌐 `vm packages auth --github` validates and imports the active GitHub CLI credential into controller-only storage without printing it or exposing it to project workers.
- 🌐 Flat package source shelves can mark managed tool repositories with `vm-tool.yaml`, preventing recursive language-package registration from misclassifying them.
- 🌐 Managed tool downloads receive read credentials over standard input, and collections merge into existing skill roots without replacing personal or system skills.
- 🌐 Package reconciliation installs the platform-matched guest `vm` client from the authenticated appliance, verifies its digest, preserves managed-guest identity through shell launchers, and binds scoped credentials to the selected environment's project.
- 🌐 Managed package reconciliation activates installed Node and Cargo toolchains for non-interactive release checks and refreshes host Git author identity in existing guests.
- 🌐 Resuming a durable package release now reacquires an expired active-checkout lease instead of failing its submission upload.
- 🌐 Permanent release preflight failures now restore compacted source and return the checkout to its assigned agent with actionable rework instead of retrying forever; package and tool manifest version bumps alone remain patch-level while other manifest changes stay public.
- 🌐 Review workers download immutable submission bundles instead of mounting workflow checkout storage, and transient appliance import trees are compacted after bundling.
- 🌐 Invalid nested built-in commands remain with their owning CLI parser instead of falling through to remote-command namespace resolution.
- 🌐 Resubmitted package generations now receive distinct validation, review, and integration operations, and `vm packages release` resumes a durable submitted generation instead of waiting indefinitely.
- 🌐 Canonical binary releases ignore untracked runtime files, retry infrastructure-caused build failures at the same source commit, clear stale phase receipts, and route isolated Cargo dependencies through the private registry.
- 🌐 Docker package edges initialize cache ownership before dropping privileges, preserve running environment image identity during reconciliation, use project-specific gateway names, and accept decoded scoped npm tarball routes.
- 🌐 Canonical workspace releases review the full tree initially and every commit since the last internal publication thereafter, while deterministic binary-build failures return actionable rework instead of hot-looping.
- 🌐 Managed binary activation replaces matching unmanaged executables and moves different bytes into a receipted guest-local backup before linking the release.
- 🌐 `vm packages doctor --fix` repairs the activation worker, sidecars, trusted-source registration drift, and interrupted rollout state.
- 🌐 Docker image pulls retry transient transport failures while permanent authentication and image errors fail immediately.
- 🌐 Single-port ranges and explicit create-time port mappings now validate correctly.
- 🚀 Tart stop operations are idempotent, and resolved stopped guests start reliably before shell connections.
- 🚀 Docker client and service provisioning no longer depends on Python APT bindings and avoids reinstalling tools that are already present.
- 🚀 Generated zsh configuration initializes its prompt-hook array before testing membership, eliminating first-login math errors.
- 🚀 Managed shell configuration keeps `yoclaude` and `yocodex` available and reports the Vibe-base recovery path when an older environment lacks their binaries.
- 🚀 Docker and Tart Vibe builds keep Codex's complete standalone runtime outside host-synced state, including the required code-mode helper; Docker derived images also invalidate when their base changes and build through the current snapshot API.
- 🚀 Docker environment discovery excludes managed service containers, while creation safely reuses only the exact host ports owned by preserved services.
- 🚀 `vm run` applies the same package client environment as other create paths.
- 🚀 Tart guest-home sync and mount paths expand the intended guest home instead of creating literal `$HOME` paths.
- 🚀 Missing standard Tart vibe bases are built by the installed binary when environment creation needs them, including through `vm ssh` and on another configured disk.
- 📦 Preset-backed `vm config unset` operations materialize the effective preset before removal, so deleted profiles and fields stay removed.
- 📦 Empty Docker inspection results now report an error instead of returning a misleading default status.
- 📦 Release and onboarding checks now follow the supported GitHub binary/source installation path instead of broken crates.io packaging.

### ⚡ Performance

- ☁️ Fingerprinted bootstrap skips locked dependency and browser installation when the relevant inputs have not changed.
- ☁️ Provider-neutral Cargo, Node, Go, Python, uv, Corepack, npm, and Playwright caches persist across Docker recreation and remain off source binds in every provider.
- 🚀 Tart provisioning batches ordered guest work and reuses one host-detected project plan, reducing SSH round trips and repeated filesystem probes.
- 🚀 Shell startup defers NVM loading, while versioned home repair and AI installers skip work when the guest state is current.
- 🚀 Host package discovery performs one Cargo, npm, or pipx inventory lookup per package manager instead of one subprocess per requested package.

### 🔒 Security

- 👤 Authentication proxy checks use constant-time token comparison, bounded request bodies, atomic secret persistence, and rejection audit logs without exposing tokens.
- ☁️ Snapshot imports reject absolute paths, traversal entries, symlinks, and hardlinks.
- ☁️ Snapshot and plugin storage rejects traversal in names and manifest-controlled filenames before resolving or deleting paths.
- ☁️ Database backup filenames reject traversal, and PostgreSQL identifiers are quoted before administrative SQL executes.
- ☁️ Temporary host mounts resolve symlinks before validation and reject paths that escape into protected host locations.
- 🌐 The package registry binds to loopback by default, requires bearer authentication for remote binds, and safely keys npm metadata filenames.
- 🌐 Docker-published application and service ports honor `vm.port_binding`, defaulting to loopback.
- 🚀 Tart host and guest commands use one canonical quoting path for package, mount, copy, shell, and provisioning values so config cannot become shell syntax.
- 📦 Copied environment configuration is readable only by its container owner.
- 📦 Managed guests receive consumer-bound package capabilities without Git or registry-write credentials.

### 🏠 Internal

- 📦 CLI command ownership, provider lifecycle contracts, instance-state queries, and target resolution were consolidated behind smaller explicit boundaries.
- 🚀 Tart provisioning is separated into host sync, package/runtime, service, shell, and AI-tool modules.
- 🚀 Docker and Tart share project detection, Node bootstrap, cache policy, home repair, shell configuration, stable naming, and guest command helpers.
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
