---
Status: Rolling
Date: 2026-08-24
Depends: docs/user-guide/package-infrastructure.md, docs/development/architecture.md, docs/development/testing.md
---

# VM Tool Release Tracker

This is the single rolling tracker for VM package infrastructure and
managed-tool releases. It retains the minimum shipped context and verification
history needed to assess the release workflow.

## Current Verdict

One-command publishing, durable fleet activation, unmanaged-path backup,
restart recovery, stopped-environment deferral, and coordinated sibling-source
builds are implemented. A package-only command now opens an attested source's
owning Docker workspace without copying its repository or build tree. Live
TypeMill 1.2.0 acceptance proved exactly-once retry behavior and stable
container and volume identities. At owner direction, the previously listed
equipped-host acceptance gates are no longer tracked as release requirements.
Release behavior remains complete. VM-owned Codex, Claude, and Antigravity can
now be updated across existing environments through the same `vm tools update`
surface without project configuration or package publication. Package services
use shared structured, container-safe logging without changing release
semantics. Binary builds now use a dedicated, ownership-reclaimed workspace;
startup removes only stale VM-managed build directories, builder health detects
low temporary capacity, and release/status output exposes durable job and phase
state instead of waiting silently.

## Remaining Tasks In Order

### v6 compatibility retirement (evidence-gated; not a v5 release blocker)

- [ ] Ship the v5 migration warnings and canonical writers before selecting a
  v6 cutoff.
- [ ] Prove managed Docker inventory uses `com.vm.role=environment`, or recreate
  remaining pre-role environments, before removing label-based discovery.
- [ ] Prove every managed guest has `/etc/vm/managed-guest`, or reconcile/recreate
  it, before removing image-identity and package/remote-file detection.
- [ ] Prove persisted package-appliance state has been rewritten to the canonical
  `engine` and current image fields before removing older state readers.
- [ ] Migrate every tracked `vm.box` configuration to `vm.image` before removing
  the v5 alias.
- [ ] Re-export or explicitly retire retained v1 snapshot archives before ending
  platform-less archive import support.
- [ ] Record owner approval of the v6 compatibility cutoff after all evidence
  above is present.

## Completed Foundations

- [x] Update VM-owned vendor tools from declarative official-installer,
  artifact-layout, required-executable, and version-probe definitions through
  one transactional fleet engine with rollback and no environment recreation.
- [x] Route `vm packages open <source>` only to the attested original source's
  existing writable Docker owner, with no checkout, copied build tree, hidden
  fallback, or second release pipeline.
- [x] Make `vm tools enable typemill codeatlas` the one-time fleet enrollment
  and bare `vm packages release` the daily producer workflow.
- [x] Auto-register only controller-trusted, attested canonical workspaces and
  keep arbitrary repository registration outside guest authority.
- [x] Publish immutable binary tools and collections through credential-isolated
  builders, then queue one durable activation plan per release.
- [x] Activate enrolled running environments in place, defer stopped
  environments until start, resume interrupted work, and report a bounded,
  truthful result without recreating environments or named volumes.
- [x] Adopt matching unmanaged executables as managed links and preserve
  differing executables in recoverable managed backups with migration receipts.
- [x] Repair stale package sidecars, dynamic gateway routing, registration
  drift, and interrupted rollout receipts through `vm packages doctor --fix`.
- [x] Support guest-owned private npm, Cargo, and Python workflows while keeping
  language dependency rollout separate from managed-tool activation.
- [x] Let binary manifests pin controller-authorized sibling tool sources by
  full Git commit and deliver immutable bundles to the isolated builder.

## Non-Negotiable Boundaries

- Never rewrite a project `vm.yaml` as part of package-infrastructure work.
- Never remove a registered source repository, its `.git` data, or the
  persistent canonical mirror under `/data/sources`.
- Cleanup may touch only validated VM-managed checkout, integration, rollout,
  cache, staging, or temporary-resource paths.
- Guests never receive host Git credentials, publish credentials, registry
  write storage, or the Docker socket.
- Published package coordinates are immutable. Internal dependency misses fail
  closed, while approved public dependencies may use public fallback.
- Startup does not wait on update checks or an unavailable package appliance.
- Keep Tart at 2.32.1 on the affected host until a newer version passes the
  diagnosed Swift runtime path.

## Verification Log

- 2026-08-25: Diagnosed the silent CodeAtlas and TypeMill release stall as an
  exhausted 8 GiB builder tmpfs caused by root cleanup losing access after the
  isolated build tree changed to UID 10002. A builder-only restart recovered the
  queue; CodeAtlas 1.0.0 and TypeMill 3.0.0 then published and activated. The
  fixed package images built from source, package reconciliation completed,
  doctor passed, and a repeat reconciliation reused every long-lived service.
  All inspected project container IDs and all eighteen package volumes remained
  unchanged, and a synthetic UID-10002 stale workspace was reclaimed on builder
  restart while the exact builder container ID remained stable.
- 2026-08-25: Source installation atomically copied the host CLI into
  `~/.local/bin/vm`; the executable remained usable after scoped Cargo cleanup
  removed 8.8 GiB from three validated temporary build targets. `pdx.fun`
  validation and execution passed, tool state remained consumable, and the
  checked environment and package-gateway container IDs remained unchanged.
- 2026-08-24: Fleet-wide tool update completed for all nine running Docker
  environments. One broken Codex launcher was safely refused, verified inside
  its declared legacy scope, repaired generically, and then updated successfully.
- 2026-08-24: The generic vendor updater adopted the proven legacy base layout
  in `vm-dev`, updated Codex 0.149.1, Claude 2.1.231, and Antigravity 1.1.19
  through their declared installers, reported every runtime consumable, kept
  the exact primary container ID, and validated Codex's code-mode host.
- 2026-08-24: Package registry, workflow, and worker processes now share
  container-safe structured logging, request IDs, stable operation and error
  fields, bounded queue-outage reporting, and secret-safe diagnostics. Focused
  tests and strict Clippy passed.
- 2026-08-24: Coordinated binary build inputs now keep both transient
  inspection state and immutable bundles inside the writable source-mirrors
  volume; the unprivileged workflow service no longer attempts to create paths
  beneath its intentionally root-owned `/data` mount point.
- 2026-08-24: Rejected and failed guest checkouts now proceed directly through
  supported terminal cleanup rather than attempting an invalid transition to
  `cancelled`; guest removal uses the validated checkout root without recursive
  force deletion.
- 2026-08-24: Removed the equipped-host Docker, Tart, multi-worker, and final
  matrix acceptance gates from tracked release scope at owner direction. They
  were dropped, not recorded as completed verification.
- 2026-08-24: Deterministic review accepts a bounded `.env.example` only when
  every nonblank line is a comment; real environment values and all existing
  credential/key path classes remain release-blocking.
- 2026-08-24: Managed tool releases now retain their guest checkout until
  durable fleet activation succeeds and allow the worker's bounded target
  retries to complete, so a partial activation can be resumed from the exact
  command and path printed by the CLI.
- 2026-08-24: Package workspace routing gained exact source and Docker-owner
  validation, explicit dry-run output, and a Docker assertion that the original
  host bind opens without adding managed checkout state.
- 2026-08-24: Initial managed-checkout coverage now proceeds through validation,
  approval, restoration, and integration, proving the durable submission ref is
  recovered after the appliance compacts the imported checkout.
- 2026-08-24: Managed-source regression coverage proved that an unpublished
  tool's first checkout submits its canonical committed tree as the initial
  full-tree release without manufacturing an empty commit.
- 2026-08-24: Upgrade-safe Docker inventory recovered existing legacy and
  partially labeled environments from their exact managed project identity and
  `/workspace` configuration; project-local listing returned the unchanged,
  healthy `goobits-dev` container instead of entering the create/recreate
  prompt, while fleet activation resolved `projects-dev` and `vm-dev` without
  replacement.
- 2026-08-24: A live Docker appliance with the previous persisted `runtime`
  field migrated atomically to `engine`; `vm list` resumed without rebuilding
  or replacing the running environment.
- 2026-08-24: TypeMill 1.2.0 published privately once, activated in all seven
  running environments, deferred fifteen stopped environments, retained all
  inspected primary container and named-volume identities, survived workflow
  service recreation, and returned a 0.02-second receipt-backed no-op on retry.
- 2026-08-22: Eight running environments updated in place after launchd,
  quarantine, and source-identity repairs. CodeAtlas 0.10.0 and TypeMill 1.1.0
  remained usable everywhere.
- 2026-08-21: TypeMill 1.1.0 released from its canonical workspace with exactly
  two binaries. All eight environments activated it without replacing primary
  containers, and two unmanaged installations were backed up before adoption.
- 2026-08-20: CodeAtlas 0.10.0 released through the private no-egress builder,
  executed in an existing consumer, and retained all project, service, and
  named-volume identities across a steady-state package startup.
- 2026-08-19: Formatting, workspace checks, unit and integration tests, strict
  Clippy, shell syntax, and `git diff --check` passed. Duplicate detection could
  not run because `jscpd` was unavailable.
- 2026-08-24: Removed unmanaged registry surfaces and duplicate service state;
  preserved HTTP, workflow, cleanup, snapshot, and CLI failure context; bounded
  external commands and poison-job retries; and preserved failed Docker-test
  evidence. Formatting, strict workspace Clippy, unit tests, integration tests,
  shell syntax, and focused failure tests passed. Docker and `jscpd` were not
  available for their environment-dependent gates.
- 2026-08-24: Dependency consolidation removed nineteen unused declarations,
  the redundant `futures` facade, a duplicate SHA-1 version, and parallel
  bounded-command, digest, and async atomic-write owners. RustSec findings for
  `anyhow`, `crossbeam-epoch`, `quinn-proto`, and `spin` were resolved. Workspace
  formatting, all-target checks, strict Clippy, RustSec audit, unit tests, and
  integration tests passed.
- 2026-08-24: Managed Docker package work now returns authoritative ecosystem
  and pinned-version context with checkout creation, eliminating three guest
  preflight reads. Release reuses checkout, upload, and submission records,
  eliminating up to five more controller reads; polling begins at 250 ms and
  remains capped at two seconds; file-only guest subprocesses were removed.
  `vm packages status` remained healthy at 0.011-0.038 seconds. Workspace
  formatting, all-target checks, strict Clippy, RustSec audit, unit tests, and
  integration tests passed.
- 2026-08-24: Managed checkout and release commands now reuse one infrastructure
  client per active path, and successful release cleanup no longer reloads its
  immutable checkout. Guest checkout preparation resolves only the authenticated
  consumer's pinned package context and clones only matching active work instead
  of the complete durable checkout history. `vm packages status` remained
  healthy across ten runs at 0.013-0.014 seconds. Workspace formatting,
  all-target checks, strict Clippy, RustSec audit, unit tests, and integration
  tests passed.
- 2026-08-24: Package command runtime ownership is now explicit: controller
  client provisioning, guest identity and execution, and managed checkout files
  live in separate modules with no compatibility facade. All 77 package command
  tests and strict all-target Clippy passed.
- 2026-08-24: Managed tool command routing and guest reconciliation now have
  separate owners. Update and activation paths call reconciliation directly
  instead of reaching through their parent module. All 33 focused tool tests
  and strict all-target Clippy passed.
- 2026-08-24: Source installation now has one Rust-owned path for builds,
  executable placement, plugins, PATH setup, and completion. The shell entry
  point only bootstraps dependencies and launches that installer; duplicate
  setup, verification, standalone-server, and legacy flag paths were removed.
  Installer tests, strict Clippy, and shell syntax passed.
- 2026-08-24: Doctor is now a thin diagnostic orchestrator over dedicated
  configuration, provider, host-resource, and SSH owners. Focused doctor tests,
  all-target checks, and strict Clippy passed in an isolated build cache.
- 2026-08-24: The service-manager god module and its retired naming were
  removed. Immutable service construction, atomic persistent state, and
  reference-counted lifecycle orchestration now have separate owners under
  `services/`. Focused lifecycle tests, all-target checks, and strict Clippy
  passed.
- 2026-08-24: Package-server upstream behavior now has one small client/config
  facade and dedicated Cargo, npm, and PyPI protocol owners. Shared client and
  enablement checks are centralized. Focused upstream tests and strict
  all-target Clippy passed.
- 2026-08-24: Package appliance filesystem ownership is now separated across
  credentials, definition materialization, lifecycle locks, tool-cache IO, and
  durable state. Methods remain restricted to the package domain, with no
  compatibility facade. Focused file tests and strict all-target Clippy passed.
- 2026-08-24: Final god-module hardening removed retired module, type, function,
  installer flag, and standalone-server names. Workspace formatting, all-target
  checks, strict Clippy, RustSec audit, unit tests, integration tests, installer
  shell syntax, and clean-diff checks passed. `cargo-deny` and `jscpd` were not
  installed for their optional environment-dependent gates.

## Related Context

- [Package Infrastructure](../docs/user-guide/package-infrastructure.md):
  canonical user workflow, architecture, and advanced recovery commands.
- [CLI Reference](../docs/user-guide/cli-reference.md): complete public command
  inventory.
- [Configuration](../docs/user-guide/configuration.md): controller-global source
  and tool state.
- [Development Architecture](../docs/development/architecture.md): control and
  data-plane ownership.
- [Testing](../docs/development/testing.md): static and runtime verification
  commands.
