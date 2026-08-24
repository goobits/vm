---
Status: Rolling
Date: 2026-08-24
Depends: docs/user-guide/package-infrastructure.md, docs/development/architecture.md, docs/development/testing.md
---

# VM Tool Release Tracker

This is the single remaining-work tracker for VM package infrastructure and
managed-tool releases. It owns only unfinished release acceptance and the
minimum shipped context needed to run those checks safely.

## Current Verdict

One-command publishing, durable fleet activation, unmanaged-path backup,
restart recovery, stopped-environment deferral, and coordinated sibling-source
builds are implemented. The remaining release adds an explicit package-only
route into an attested source's owning Docker workspace without copying its
repository or build tree. Live TypeMill 1.2.0 acceptance proved exactly-once
retry behavior and stable container and volume identities, but Docker daemon
interruption and live unmanaged-binary adoption remain unproven. Final Docker,
Tart, multi-worker, and full-matrix host gates remain before release readiness.

## Remaining Tasks In Order

The package workspace route can be implemented and statically verified here.
The later acceptance phases require an equipped macOS host with Docker, Tart
2.32.1, the TypeMill workspace, and a second Docker worker. Neither Docker nor
Tart is available in the current development container.

### Phase 1: Package Workspace Routing

- [ ] Add `vm packages open <source>` as a controller-owned, package-specific
  route to the attested source's existing `/workspace` in its owning managed
  Docker environment. Do not create a checkout, copy source, or copy build
  output.
- [ ] Keep `vm packages checkout <source>` as the explicit isolated workflow.
  Share source validation and the existing canonical-workspace release path;
  never silently fall back between the two modes.
- [ ] Cover command parsing, source and owner resolution, dry-run output, and a
  Docker acceptance assertion that opening a workspace creates no managed
  checkout state.

### Phase 2: Docker Release Acceptance

- [ ] Pass the real TypeMill Docker daemon-interruption and live
  unmanaged-binary-adoption scenarios. Confirm activation resumes, the prior
  executable is recoverably backed up, a repeat release is an immediate no-op,
  and every primary container and named-volume ID remains unchanged.
- [ ] Run the extended Docker package-workflow acceptance test on an equipped
  host. Prove a source-only language-package release and failed-then-retried
  cancellation restore local dependencies before durable closure, without
  recreating containers or volumes.
- [ ] Host-accept steady-state package startup, concurrent shell
  reconciliation, targeted tool updates, read-only workspace restart, and
  first-shell `codex-code-mode-host` availability.

### Phase 3: Provider And Multi-Worker Acceptance

- [ ] On an equipped macOS host, run the Tart path in
  `validate-vibe-providers.sh`. Verify managed inventory, shell recovery,
  worktree mounts, package-edge repair, and exact `--to` updates from the same
  `vm.yaml` without replacing the VM or its Tart storage.
- [ ] From a second Docker worker, verify resumable guest-owned npm, Cargo, and
  Python release and rollout, private immutable artifacts, per-worker
  overrides, public fallback, persistent caches, and fail-closed internal
  misses during an appliance outage.

### Phase 4: Final Release Gate

- [ ] Run the complete build, lint, unit, integration, duplicate-detection, and
  Docker matrix after the equipped-host checks pass.

## Completed Foundations

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

- 2026-08-24: Initial managed-checkout coverage now proceeds through validation,
  approval, restoration, and integration, proving the durable submission ref is
  recovered after the appliance compacts the imported checkout.
- 2026-08-24: Managed-source regression coverage proved that an unpublished
  tool's first checkout submits its canonical committed tree as the initial
  full-tree release without manufacturing an empty commit.
- 2026-08-24: Legacy-label Docker inventory recovered the existing
  `goobits-dev` environment from its exact managed project identity and
  `/workspace` configuration; project-local listing returned the unchanged,
  healthy container instead of entering the create/recreate prompt.
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
