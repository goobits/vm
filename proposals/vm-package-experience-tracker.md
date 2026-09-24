# VM Package Experience Tracker

Status: implementation and local verification complete; Docker acceptance pending.

## Remaining work

- [ ] Run [the Docker acceptance workflow](../scripts/internal/test-package-workflow-docker.sh)
      against the updated code on a host with Docker available.
- [ ] Record the result and resolve any failures before closing this tracker.

Docker is unavailable in the audit environment. The previous Docker acceptance
passed on 2026-08-28; that result predates the latest changes and does not validate
them. Tart acceptance remains outside this tracker's scope.

## Completed work

The release implementation and 2026-09-24 package workflow audit are complete.
Repairs cover registry routing and Cargo metadata, retained publication artifacts,
checkout/integration recovery, consumer update retries, queue fairness, and cleanup.
Operational behavior is documented in the
[package infrastructure guide](../docs/user-guide/package-infrastructure.md).

Verification on 2026-09-24:

- 425 tests passed across `vm-packages`, `vm-package-work`, `vm-package-jobs`,
  `vm-package-server`, and `goobits-vm` with
  `--all-features --lib --bins --locked --offline`.
- Four additional local npm/Cargo fixtures passed: fresh-source dependency setup,
  integration failure restoration, prepack build dependencies, and installation
  from a local Cargo registry with a renamed private dependency.
- Clippy passed for those packages with
  `--all-features --all-targets --locked --offline -- -D warnings`.
  Workspace formatting and diff checks passed.

## Guardrails

- Extend the existing submission, build, release, activation, source-discovery,
  and appliance-reconciliation owners; do not create parallel job concepts.
- Derive behavior from manifests, lockfiles, tool kind, target, provider, and
  configured sources. Never branch on a package or tool name.
- Keep the package appliance without a Docker socket or project source mounts.
- Keep public registry publication and Tart acceptance out of scope.

## Acceptance

- One release command; no routine IDs or flags.
- Output starts within two seconds and remains live at least every ten seconds.
- A warm mixed Node/Rust tool build completes without shared writable build
  output between jobs.
- Running environment activation is bounded and concurrent; stopped targets are
  deferred.
- Repeated release and repair commands are receipt-backed no-ops.
- Primary project container IDs and package named-volume IDs remain unchanged.

## Current scope limits

- Consumer versions require registration after reviewed updates; automatic Git
  merge detection and monorepo-wide consumer discovery are not implemented.
- Language validation follows npm and pip/pytest conventions. pnpm/Yarn-specific
  editable installs, Yarn PnP, Poetry-specific consumer updates, and dependencies
  outside root manifests need separate acceptance before claiming support.
  Existing tool-builder lockfile support is unchanged.
- Python releases require a static stable semantic version; dynamic metadata and
  broader Python version schemes are outside the current release contract.
