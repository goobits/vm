---
Status: Reconciliation implemented but awaiting host acceptance
Date: 2026-08-12
Depends: docs/user-guide/package-infrastructure.md, docs/development/architecture.md
---

# VM Tool Completion Tracker

This is the single remaining-work tracker for the active VM-tool release. The
base implementation and scoped first-run/upgrade reconciliation are complete.
Host behavior is implemented but awaiting host acceptance before the remaining
live Docker and Tart matrix is closed.

Package development and release work runs in Docker or Linux VMs. The Mac only
launches those runtimes and holds controller credentials; it does not build,
test, merge, or publish package source directly.

## Non-Negotiable Boundaries

- Never stage or rewrite the user's `vm.yaml` as part of implementation commits.
- Never remove `/workspace`, a registered source repository, its `.git` data,
  or the persistent canonical mirror under `/data/sources`.
- Cleanup may remove only validated VM-managed checkout, integration, rollout,
  cache, staging, or temporary-resource paths.
- Project and integration agents never receive host Git credentials, publish
  credentials, or writable registry storage.
- A published package coordinate always returns identical bytes.
- Internal dependencies fail closed. Public fallback is allowed only for
  packages classified as external.
- Startup does not wait on update checks or an unavailable package appliance.
- Keep Tart at 2.32.1 on the affected host until a newer release works without
  the diagnosed Swift runtime failure.

## Completed Implementation

### Worker package data plane

- [x] Give Docker and Linux Tart workers one read-only package edge with native
  npm, Cargo, and Python endpoints and persistent cache.
- [x] Use one shared resolver policy and separate internal/public caches.
- [x] Cache the last-known-good package catalog with bounded, nonblocking
  refresh and transition-based outage logging.
- [x] Permit approved public upstream fallback only for known external
  packages; fail closed before the first catalog or for internal cache misses.
- [x] Exclude publish, package-work, and tool-write routes and credentials from
  the worker edge.
- [x] Make the Docker sidecar restartable without rebuilding the project image
  and provision the same contract in Linux Tart's Docker Engine.

### Durable package-development overrides

- [x] Persist checkout identity, consumer, ecosystem, source, assignment, and
  restoration state.
- [x] Apply checkout-scoped npm local-source, Cargo patch, and Python editable
  overrides without publishing mutable bytes under an existing version.
- [x] Refuse silent fallback when an assigned worktree is missing.
- [x] Restore published dependency configuration before removing checkout data.
- [x] Validate managed paths centrally and constrain cleanup to task-owned data.
- [x] Remove agent and integration worktrees after successful integration or
  publication while retaining durable receipts and required immutable data.

### Tart lifecycle and shell recovery

- [x] Route Tart commands through one storage-aware context and persist managed
  instance ownership in `~/.vm/tart/instances.json`.
- [x] Make base pull/build replacement transactional and retain a known-good
  base when acquisition or validation fails.
- [x] Preserve the configured Rust version while building a local base.
- [x] Keep `vm exec` strict on the guest agent and make `vm ssh` wait for shell
  readiness with one real 60-second deadline.
- [x] Prefer `tart exec`; for macOS guests only, verify native SSH and use the
  managed `~/.vm/ssh/tart_ed25519` identity as a bounded fallback.
- [x] Install the controller public key during provisioning and support one
  password bootstrap for an existing guest, followed by key-only connections.
- [x] Detect and remount missing VirtioFS shares while preserving each mount's
  source and read-only/read-write access.
- [x] Warn when concurrent Tart and Docker allocations materially oversubscribe
  the host without rewriting explicit resource limits.

### Provisioning and runtime reliability

- [x] Keep Antigravity, Claude Code, and Codex in Docker and Tart Vibe bases;
  reserve managed-tool activation for `agent-skills` and other explicit tools.
- [x] Keep Codex's immutable executable outside host-synced `~/.codex` state,
  preserve its canonical package and code-mode helper beside it, and include
  the base image ID in derived-image cache keys.
- [x] Build Docker Vibe bases through the current snapshot API, exclude managed
  service containers from environment discovery, and reuse only their exact
  occupied host ports during environment creation.
- [x] Replace duplicated AI-sync templates with one valid policy covering
  boolean and granular Claude, Codex, Antigravity, and legacy Gemini settings.
- [x] Accept Tart 2.32.1's capitalized VM inventory fields when starting the
  shared package appliance.
- [x] Let Docker appliance acceptance use locally built immutable image
  overrides without forcing a remote pull.
- [x] Keep local build outputs and dependency directories out of package-image
  Docker contexts.
- [x] Keep package-image Cargo outputs inside the builder stage even when the
  host workspace redirects its target directory.
- [x] Build package-job images against current Node bases without conflicting
  with their preinstalled Corepack package-manager shims.
- [x] Publish the gateway through a dedicated controller bridge while keeping
  registry and workflow services isolated on the internal appliance network.
- [x] Persist explicit appliance image overrides for same-version restarts and
  return to matching release images after a controller upgrade.
- [x] Fall back to Docker-native local image builds for discoverable source
  installs when matching unreleased images are not pullable.
- [x] Recheck source-built appliance images through Docker's content-addressed
  build cache so service- or job-only edits cannot leave stale local images.
- [x] Resolve source-installed CLI symlinks and initialize package-volume roots
  before non-root services start, including volumes introduced by upgrades.
- [x] Publish registered collections through a credential-isolated ephemeral
  job and bootstrap the built-in `agent-skills` definition from `vm tools
  update`; keep initial publication an explicit operator action.
- [x] Deliver artifact read credentials to running guests over standard input,
  and merge collection skills without replacing existing agent skill roots.
- [x] Define the shared publish secret explicitly for ephemeral package and
  tool release jobs so Compose mounts the intended read-only token file.
- [x] Let operators validate and explicitly import the active GitHub CLI
  credential into controller-only storage without printing or forwarding it to
  workers.
- [x] Let one flat host source shelf mix language packages and explicitly marked
  tool repositories without hardcoded names, paths, or accidental npm
  registration.
- [x] Keep tool update discovery off the interactive startup critical path.
- [x] Bound streamed commands, terminate and reap timed-out children, cap error
  output, and keep broken pipes from panicking the CLI.
- [x] Diagnose host file-descriptor pressure and allocation oversubscription.
- [x] Scope temporary-resource cleanup to explicit VM ownership labels.

### Security hardening

- [x] Reject unsafe snapshot archive entries and preserve known-good snapshots
  until staged import/replacement succeeds.
- [x] Require appropriate package-server authentication and safe binds, validate
  decoded names and paths, and enforce immutable artifact writes.
- [x] Keep Tart host command construction structural rather than interpolating
  configuration into host shell commands.
- [x] Validate plugin names and stage database restore before replacement while
  quoting database identifiers and literals.
- [x] Create and repair managed secret directories/files as 0700/0600, refuse
  symlinks, and avoid duplicated sync implementations.
- [x] Bind managed databases to loopback, compare actual runtime configuration,
  and share one labeled service lifecycle implementation.
- [x] Remove global image/build-cache pruning from ordinary cleanup.
- [x] Keep test fixtures in unique temporary locations.
- [x] Require and verify release SHA-256 files before replacing the installed
  CLI, including correct Windows release assets.

### Documentation and repository structure

- [x] Document worker-edge outage behavior, cache rules, restart versus rebuild,
  checkout cleanup, and source-code safety.
- [x] Document Tart storage ownership, SSH bootstrap, mount recovery, resource
  pressure, and the affected-host version constraint.
- [x] Record clear crate ownership and the package control/data-plane boundary.
- [x] Keep package-infrastructure scope in the root agent instructions and this
  one canonical tracker.

## Implemented First-Run And Upgrade Reconciliation

- [x] Make `vm packages up` preflight and reconcile configurable
  controller-wide source roots, accepting an empty configured shelf without
  weakening strict manual registration.
- [x] Keep tool publication explicit and report registered, published,
  installed, and consumable state separately, including stale controller or
  guest rows.
- [x] Reconcile a missing or stale worker package edge without rebuilding its
  base or recreating unrelated services, and version edge runtime policy
  independently from the registry image.
- [x] Repair an incomplete existing Codex standalone runtime without writing
  through host-synced `~/.codex` state, overwriting unmanaged launchers, or
  leaving a partial package/link transaction.
- [x] Detect managed guest context and print the exact command to run on the
  controller host.
- [x] Cover fresh setup and existing-machine reconciliation with fake providers
  and temporary fixtures, then synchronize command help and user documentation.

All host-facing behavior in this section is implemented but awaiting host
acceptance. Container tests cover fake Docker/provider execution, temporary
controller state, fixture source discovery, repeat reconciliation, targeted
sidecar updates, and volume-preserving command construction.

## Static Verification

The container-safe verification gate is:

```bash
make fmt
cd rust && cargo check -j 1 --workspace --all-features
make test-unit CARGO_JOBS=1
make test-integration CARGO_JOBS=1
make clippy CARGO_JOBS=1
make check-duplicates
git diff --check
```

No Docker image, Tart base, guest, or release binary is built by this gate.

Latest result on 2026-08-12: formatting, the serial all-feature workspace check,
the complete unit suite, the supported non-network integration suite, and
`git diff --check` passed from an isolated container-local Cargo target. The
integration wrapper now enables the package server's declared
`standalone-binary` feature in both runner paths. Scoped package/tool Clippy
passed with warnings denied. Full workspace Clippy remains blocked by
pre-existing Tart storage warnings and the existing unused Linux doctor parser.
Duplicate detection could not run because `jscpd` is not installed in this
container. No Docker, Tart, host VM, network, or publication action was run.

## Post-Recreation Acceptance

The first Docker shell smoke test exposed and fixed an uninitialized zsh prompt
hook, then revealed that standard AI CLIs had been incorrectly coupled to the
private package appliance. Vibe bases again own Antigravity, Claude Code, and
Codex; only `agent-skills` remains selected through `vm tools`. Tart appliance
startup now accepts the inventory format returned by Tart 2.32.1. Full package
appliance acceptance remains below.

Docker appliance startup now succeeds with the source-installed CLI, remains
healthy on a no-flags restart from outside the project, and retains its named
volumes. The rebuilt Docker Vibe base now creates `vm-dev` while reusing its
preserved PostgreSQL service, `yocodex` resolves Codex 0.147.0 outside synced
state, and `agent-skills` 0.6.1 activates 26 skills across all five supported
agent locations without replacing Codex system skills. The flat source shelf
registered 13 npm packages while routing `agent-skills` to `vm tools`. These
are the only remaining tasks:

The new first-run/upgrade reconciliation is implemented but awaiting host
acceptance; the earlier live results below do not constitute acceptance of this
new flow.

Validate it non-destructively from the controller host with an existing
environment name:

```bash
VM_ACCEPT_EXISTING='actual-existing-environment'
vm config get packages.source_roots --global
vm packages up
vm packages up
vm packages doctor
vm packages list
vm tools list
vm tools status "$VM_ACCEPT_EXISTING"
vm tools update "$VM_ACCEPT_EXISTING" --all
vm tools update "$VM_ACCEPT_EXISTING" --all
vm tools status "$VM_ACCEPT_EXISTING"
```

If `agent-skills` is registered but unpublished, confirm that the first update
prints `vm tools publish agent-skills`, run that explicit publication command,
and then resume the two update calls. Do not publish automatically as part of
acceptance.

The first container-local Codex agent then exposed that the base installer had
copied only the main Codex executable out of its canonical standalone package.
Docker and Tart Vibe installers now preserve the complete package, including
the matching `codex-code-mode-host`, so code-backed agent tools do not fail
before their first command.

- [x] Start and restart the central package appliance in Docker from outside a
  project directory.
- [x] Populate `goobits/agent-skills` from the clean local submodule history,
  publish 0.6.1, and activate the collection in the running Docker worker.
- [ ] Start a separate Docker worker on the same managed network.
- [ ] Prove npm, Cargo, and Python public proxying, immutable internal artifacts,
  per-worker override isolation, persistent-cache restart recovery, and clear
  uncached-internal failure.
- [ ] Stop the central appliance and verify warmed workers retain cached locked
  internal artifacts, known external fallback, and fail-closed internal misses.
- [ ] Verify two consecutive `vm ssh` sessions to one macOS Tart guest; only the
  first legacy bootstrap may request a password.
- [ ] Remove a test VirtioFS guest mount and verify `vm ssh` remounts it without
  deleting or replacing host source.
- [ ] Run the repository's complete build, lint, test, Docker, and Tart matrix
  only after the recreated host is stable.

## Recovery Checkpoint

Implementation commits, in order:

```text
7b122684 docs(proposals): consolidate vm tool task tracking
3ebeedd1 feat(packages): add resilient worker edge
e3e162ba feat(packages): make checkout overrides durable
9f4a4e51 fix(tart): preserve storage and base state
4fdc79bf fix(tart): honor workspace rust version
64c9d473 fix(tart): harden shell recovery
07394eb9 fix(runtime): bound provisioning resources
e06f9554 fix(security): harden managed resources
de09ccf5 docs(vm): finalize implementation handoff
6fcb243a fix(shell): initialize zsh prompt hooks
32a4f491 fix(shell): keep managed tool shortcuts available
6b94ed93 feat(runtime): complete Vibe and package bootstrap
7832d01c feat(packages): reconcile controller setup
7b9da82e feat(tools): reconcile runtime infrastructure
593dc952 test(packages): cover reconciliation workflows
cca4de6d fix(tools): harden guest reconciliation
63f39737 test(packages): enable standalone integration fixtures
5bc1491e fix(tools): make Codex repair transactional
5aa887c8 fix(packages): list published npm metadata
d56a207b fix(packages): preflight configured source shelves
67c953d9 fix(tools): reconcile stale guest state
1fedfbf9 fix(packages): version edge runtime policy
eadfb08b docs(cli): clarify collection publication
```

The user's dirty `vm.yaml` is intentionally outside these commits.

## Related Documentation

- [Package Infrastructure](../docs/user-guide/package-infrastructure.md)
- [Configuration](../docs/user-guide/configuration.md)
- [Troubleshooting](../docs/user-guide/troubleshooting.md)
- [Development Architecture](../docs/development/architecture.md)
- [Testing](../docs/development/testing.md)

## Layman's Result

- Workers use one predictable local package service and keep useful read-only
  cache when the central appliance is briefly unavailable.
- Package experiments are isolated to one worker and clean up their own task
  worktrees without touching source repositories or Git history.
- Tart discovery, base acquisition, shell recovery, and mounts follow bounded,
  storage-aware paths.
- Cleanup, secrets, database services, snapshots, and self-updates are narrower
  and safer than before.
