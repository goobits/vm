---
Status: Container implementation complete; direct package workflow host acceptance pending
Date: 2026-08-13
Depends: docs/user-guide/package-infrastructure.md, docs/development/architecture.md
---

# VM Tool Completion Tracker

This is the single remaining-work tracker for the active VM-tool release. The
base implementation and scoped first-run/upgrade reconciliation are complete in
the development container. Prior live Docker acceptance passed for source
discovery, tool state, guest updates, and data preservation. Stable source-image
identity and nonblocking shell reconciliation are implemented but awaiting host
acceptance.

Package development and release work runs in Docker or Linux VMs. The Mac only
launches those runtimes and holds controller credentials; it does not build,
test, merge, or publish package source directly.

The assigned project agent now drives one resumable `vm packages release
<checkout-id>` workflow directly against the appliance. Persistent review,
release, and rollout services derive work from durable state. Releases publish
only to the private VM gateway, and consumer upgrade branches are prepared
automatically without host synchronization or approval commands.

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

### Direct package release and consumer reconciliation

- [x] Make checkout creation, resubmission, integration, publication, and
  cleanup retry-safe against durable workflow state.
- [x] Give each managed guest a signed, consumer-bound capability without Git
  or registry-write credentials.
- [x] Let the assigned Docker or Tart agent run checkout, validation,
  integration checks, and `vm packages release` directly.
- [x] Run review, release, and rollout as persistent restartable appliance
  workers rather than host-launched one-shot jobs.
- [x] Publish npm, Cargo, and Python artifacts only to the private VM gateway;
  remove configurable CI/public release destinations and credentials.
- [x] Automatically create, test, and push one consumer upgrade branch for
  every registered project that drifts behind a private release.
- [x] Remove the public host `submit`, `integrate`, `publish`, and `rollout`
  commands and their one-shot worker paths.

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
- [x] Reject stale Docker Vibe bases before deriving a new environment when the
  complete Codex package or code-mode host is unavailable, with the exact
  non-destructive host rebuild command.
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
- [x] Mark source-built appliance images with stable metadata rather than the
  changing controller binary hash, while recognizing legacy images once.
- [x] Resolve source-installed CLI symlinks and initialize package-volume roots
  before non-root services start, including volumes introduced by upgrades.
- [x] Publish registered collections through a credential-isolated ephemeral
  job and bootstrap the built-in `agent-skills` definition from `vm tools
  update`; keep initial publication an explicit operator action.
- [x] Deliver artifact read credentials to running guests over standard input,
  and merge collection skills without replacing existing agent skill roots.
- [x] Define the shared publish secret explicitly for the package release
  service and ephemeral tool release jobs so Compose mounts the intended
  read-only token file.
- [x] Let operators validate and explicitly import the active GitHub CLI
  credential into controller-only storage without printing or forwarding it to
  workers.
- [x] Let one flat host source shelf mix language packages and explicitly marked
  tool repositories without hardcoded names, paths, or accidental npm
  registration.
- [x] Keep tool update discovery off the interactive startup critical path.
- [x] Launch base-owned Codex repair and cached automatic tool downloads as
  detached jobs during interactive shell setup, coalescing Codex work with a
  guest-local lock while keeping explicit update reconciliation deterministic.
- [x] Coalesce concurrent shell-triggered catalog refresh, Codex repair, and
  managed-tool activation at their existing ownership boundaries, reusing a
  successful pass for 60 seconds while leaving explicit commands authoritative.
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
- [x] Put the repaired standalone Codex first through managed user launchers,
  migrate legacy NVM-backed system and official standalone user launchers, and
  continue to reject arbitrary unmanaged launchers.
- [x] Detect managed guest context and print the exact command to run on the
  controller host.
- [x] Require `vm tools update` to resolve an existing environment instead of
  creating from the invoking directory's configuration, while allowing an
  exact managed environment name to resolve across project boundaries.
- [x] Keep interactive Docker startup lean by caching successful home repair
  within one CLI run, combining cached tool-state probes, avoiding unnecessary
  worktree repair, and removing the legacy job-control-producing shell hook.
- [x] Cover fresh setup and existing-machine reconciliation with fake providers
  and temporary fixtures, then synchronize command help and user documentation.

Container tests cover fake Docker/provider execution, temporary controller
state, fixture source discovery, repeat reconciliation, targeted sidecar
updates, and volume-preserving command construction. Live Docker acceptance is
recorded below.

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
the full workspace build, and all-target workspace Clippy with warnings denied
passed from an isolated container-local Cargo target. The VM suite passed 166
tests, workspace libraries passed 489 tests, and the integration feature matrix
passed 161 tests; 20 tests requiring a real runtime or performance environment
remained ignored. Focused coverage includes reconciliation locks and cooldowns,
stable source-image markers, repeat fake-Docker reconciliation, and a fatal
fake-Tart sentinel proving Docker paths did not invoke Tart. `git diff --check`
passed. Duplicate detection could not run because `jscpd` is not installed in
this container. No Docker, Tart, host VM, network, or publication action was
run.

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

Live host acceptance on 2026-08-12 installed the current source CLI, persisted
the host package shelf only in controller-global `packages.source_roots`, and
ran the reconciliation sequence against `vm-dev`. Both tool-update passes were
steady-state no-ops. Final status reported base-owned Codex and managed
`agent-skills` 0.6.1 as installed and consumable; `agent-skills` remained
registered and published. The `vm-dev` and PostgreSQL container IDs remained
`3095a72fe244` and `ff9d7cf85390`, and PostgreSQL retained
`vm_vm_postgres_data`.

Both appliance starts remained healthy, retained the package catalog and named
volumes, and rediscovered all 13 npm sources while routing `agent-skills` to
`vm tools`. The second source-installed start reused every Docker build layer,
but changing local image identity caused Compose to recreate the registry/work
containers. State safety passed; literal steady-state no-op acceptance did not.
The controller-derived image label has since been replaced by a stable
source-build marker. That fix is implemented but awaiting host acceptance.

The first container-local Codex agent then exposed that the base installer had
copied only the main Codex executable out of its canonical standalone package.
Docker and Tart Vibe installers now preserve the complete package, including
the matching `codex-code-mode-host`, so code-backed agent tools do not fail
before their first command.

Managed collection reconciliation now detects standalone Git checkouts at the
same project activation paths. Status exposes `PROJECT_COPY`, and update emits
a non-destructive ownership warning instead of implying that project submodules
are synchronized. VM continues to own guest-home activation and never mutates
mounted project Git.

Burst-shell reconciliation now holds the controller catalog lock before a
background task is launched and uses guest-local single-flight locks for Codex
and managed tools. Successful shell-triggered work is reused for 60 seconds;
explicit refresh/update commands bypass that recent-success window. This is
implemented but awaiting host acceptance.

Live Zoop acceptance on 2026-08-12 detected its legacy `.agents/skills` and
`.claude/skills` repository copies, then reported `PROJECT_COPY=no` after their
scoped Git removal. In-place `vm tools update zoop-io-dev` changed Codex
from installed/non-consumable to installed/consumable without recreating the
Docker worker, while managed `agent-skills` remained consumable at 0.6.1.

Bulk reconciliation is now exposed through ordinary command ownership rather
than a duplicate top-level workflow. `vm tools update --fleet` applies the
loaded declarative tool selection to matching managed environments, includes
prompt-policy upgrades without a checklist, respects `off` for newer releases,
starts stopped targets in place, continues on per-target failures, and reports a
summary. It does not project the invoking project's service or package-edge
configuration onto unrelated targets. The former `vm fleet` and tool `--all`
surfaces are removed. This is implemented but awaiting host acceptance.
With no managed tools selected, base-owned Codex reconciliation no longer
requires a tool catalog or package-appliance connection.

- [x] Start and restart the central package appliance in Docker from outside a
  project directory.
- [x] Populate `goobits/agent-skills` from the clean local submodule history,
  publish 0.6.1, and activate the collection in the running Docker worker.
- [ ] Host-accept repeated source-installed `vm packages up` without recreating
  registry/work containers when their effective image content is unchanged
  (implemented but awaiting host acceptance).
- [ ] Open several concurrent `vm ssh` sessions to one existing worker and
  verify only one catalog/Codex/tool reconciliation does work, then reconnect
  within 60 seconds and verify no duplicate job starts (implemented but awaiting
  host acceptance).
- [ ] Run `vm tools update --fleet --provider docker` twice and verify all
  matching workers reconcile in place, the second pass is a no-op, and no
  primary container ID or service volume changes (implemented but awaiting host
  acceptance).
- [ ] Create and restart a Docker worker with
  `project.workspace_access: read_only`; verify nested mountpoints are prepared,
  the second start is a no-op, and project source, container identity, and
  service volumes remain unchanged (implemented but awaiting host acceptance).
- [ ] Rebuild `@vibe-box`, create a fresh Docker worker, and launch `yocodex`
  immediately; verify the code-mode host is already executable without waiting
  for background reconciliation (implemented but awaiting host acceptance).
- [ ] Start a separate Docker worker on the same managed network.
- [ ] From that worker, create and release a package checkout using only the
  scoped guest commands; restart each persistent worker mid-flow and verify the
  same workflow resumes without duplicate branches, tags, or artifacts.
- [ ] Verify one npm, Cargo, and Python release reaches only the private gateway
  and automatically produces tested upgrade branches for every drifted
  registered consumer without a host command.
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
7a7129cd fix(tools): preserve partial Codex backups
77027272 docs(packages): record final transaction fix
207e06c0 fix(runtime): preserve complete Codex package
c16747bc fix(tools): report project collection overrides
b5ad97f0 feat(runtime): reconcile codex without blocking shells
fcc24a56 fix(packages): stabilize local appliance images
9fc3b84b fix(shell): coalesce background reconciliation
11ad87d3 refactor(cli): fold fleet into shared targeting flags
ad4c7a74 feat(tools): reconcile managed environments with fleet flag
6795cd9a docs(cli): replace fleet command with targeting flag
e1254dcf chore(rust): keep cross-platform checks warning-free
30ba7620 fix(tools): isolate fleet reconciliation scope
8b7fa9d1 fix(tools): decouple empty tool reconciliation
498322e8 fix(packages): make release workflows resumable
25507579 feat(packages): allow scoped guest package work
0f39138b feat(packages): run durable infrastructure workers
a6d06a41 feat(packages): automate private consumer releases
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
- An assigned agent can release through the private appliance without a host
  handoff; other registered projects receive tested upgrade branches
  automatically.
- Tart discovery, base acquisition, shell recovery, and mounts follow bounded,
  storage-aware paths.
- Cleanup, secrets, database services, snapshots, and self-updates are narrower
  and safer than before.
