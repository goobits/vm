# VM Package Experience Completion Tracker

Status: complete

## Outcome

`vm packages release` is the complete producer workflow for every registered
package or tool. It restores declared dependencies, builds once in isolation,
publishes privately, activates enabled environments, and resumes safely without
tool-specific VM code or recreating project containers and volumes.

## Guardrails

- Extend the existing submission, build, release, activation, source-discovery,
  and appliance-reconciliation owners; do not create parallel job concepts.
- Derive behavior from manifests, lockfiles, tool kind, target, provider, and
  configured sources. Never branch on a package or tool name.
- Keep the package appliance without a Docker socket or project source mounts.
- Keep public registry publication and Tart acceptance out of scope.

## Work

- [x] Repair unprivileged isolated-builder traversal and durable retry.
- [x] Restore recognized locked Node dependencies generically.
- [x] Prefer newer committed checkout work over stale immutable retries.
- [x] Record and display durable build subphases and activation progress.
- [x] Bound the existing immutable dependency cache with disk health and
      oldest-entry pruning; keep writable build outputs job-local.
- [x] Activate independent environments concurrently with per-target receipts.
- [x] Reconcile only package services affected by a source change through the
      existing server/job fingerprints and Compose identity checks.
- [x] Resolve identical source aliases automatically and report one explicit
      choice for genuinely different repositories.
- [x] Preserve the configured gateway port during routine appliance
      reconciliation unless the operator explicitly changes it.
- [x] Extend the sole Docker acceptance workflow with mixed Node/Rust builds,
      prompt output, heartbeat, workspace cleanup, controller restart coverage,
      and optional daemon restart coverage.
- [x] Consolidate package documentation under one operational guide.
- [x] Remove the drained isolated-builder compatibility retry path.
- [x] Run the Docker acceptance workflow and record its final result.

## Acceptance

- One release command; no routine IDs or flags.
- Output starts within two seconds and remains live at least every ten seconds.
- A warm mixed Node/Rust tool build completes without shared writable build
  output between jobs.
- Running environment activation is bounded and concurrent; stopped targets are
  deferred.
- Repeated release and repair commands are receipt-backed no-ops.
- Primary project container IDs and package named-volume IDs remain unchanged.

## Verification

- 2026-08-27: 384 Rust tests passed across `vm-packages`, `vm-package-work`,
  `vm-package-jobs`, and `goobits-vm` with all features.
- 2026-08-27: Clippy passed for the same four packages with warnings denied.
- 2026-08-27: Release build, shell syntax, npm/Cargo fixture, legacy-symbol,
  documentation-ownership, and diff checks passed.
- 2026-08-28: All affected Rust suites passed across `vm-packages`,
  `vm-package-work`, `vm-package-jobs`, `vm-package-server`, `vm-provider`, and
  `goobits-vm`; workspace check and scoped Clippy with warnings denied passed.
- 2026-08-28: The real Docker workflow passed source-only npm release and
  restoration, collection release, mixed Node/Rust binary builds for both Linux
  architectures, exact-version concurrent activation, controller restart
  recovery, newest-only deferred activation, unmanaged-file adoption, immediate
  receipt-backed rerelease, and unchanged primary container and named-volume
  identities.
