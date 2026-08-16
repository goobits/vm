---
Status: Guest-owned package work implemented; live Docker rerun pending
Date: 2026-08-16
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

The former host-oriented package-work launcher has been replaced by the
guest-owned workflow specified below. An agent already running
inside a managed VM requests its own checkout and runs bare `vm packages
release` from that source directory. Infrastructure never launches the agent.
The same durable workflow owns language packages and tool collections.
Credential-isolated appliance workers review, integrate, push, and publish only
to the private gateway; host working trees and installed immutable releases are
never treated as source.

## Remaining Acceptance

Implementation is complete. These are the only remaining release checks:

- [ ] Extend and rerun the sole Docker package-workflow acceptance test for a
  source-only language-package release and a cancelled checkout, proving local
  dependency restoration precedes durable closure and no container or volume
  is recreated.
- [ ] Host-accept steady-state package startup, concurrent shell
  reconciliation, targeted and fleet tool updates, read-only workspace restart,
  and first-shell `codex-code-mode-host` availability.
- [ ] From a second Docker worker, verify resumable guest-owned npm, Cargo, and
  Python release/rollout, private immutable artifacts, per-worker overrides,
  public fallback, persistent caches, and fail-closed internal misses during an
  appliance outage.
- [ ] Host-accept two consecutive macOS Tart connections and targeted VirtioFS
  remount repair without replacing guest or host source state.
- [ ] Run the complete build, lint, unit, integration, Docker, and Tart matrix
  after the host checks above pass.

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
- [x] Restore dependency configuration before closing a cancelled checkout, so
  local cleanup failures remain retryable from the assigned guest.
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
- [x] Let managed guests verify their workflow connection and scoped agent
  credential with a read-only `vm packages status`.
- [x] Run review, release, and rollout as persistent restartable appliance
  workers rather than host-launched one-shot jobs.
- [x] Publish npm, Cargo, and Python artifacts only to the private VM gateway;
  remove configurable CI/public release destinations and credentials.
- [x] Automatically create, test, and push one consumer upgrade branch for
  every registered project that drifts behind a private release.
- [x] Make rollout queue reconciliation an authenticated command and keep
  workflow routing, source control, storage, and persistence records internal
  to the workflow service.
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
  boolean and granular Claude, Codex, and Antigravity settings.
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

## Implemented Managed Collection Releases And Runtime Repair

- [x] Extend the existing managed checkout, review, integration, release, and
  receipt state machine to registered tool collections.
- [x] Remove direct collection publication and default-branch publication paths
  that bypassed assigned checkouts and integration review.
- [x] Keep npm, Cargo, Python, and collection artifacts private and immutable;
  only the credential-isolated release worker pushes canonical commits/tags and
  publishes approved artifacts.
- [x] Install root-managed, guest-readable shell, npm, pip, and Cargo settings
  atomically through standard input, refusing symlink targets and preserving
  unrelated shell configuration.
- [x] Route `vm tools update` and fleet updates through the same in-place package
  client repair instead of retaining a second reconciliation path.
- [x] Make installed tool releases read-only and replace legacy writable
  installations from their immutable private artifacts during reconciliation.
- [x] Repair missing credentials and current appliance definitions automatically
  when an existing controller predates scoped guest access; ordinary shell
  startup no longer requires a manual migration command.
- [x] Reconcile the worker edge, scoped workflow token, and package client
  profile before every interactive shell and command execution, including
  containers created before those settings existed; the signing key remains
  controller-only.
- [x] Filter guest-readable workflow, consumer, drift, and rollout records by
  the capability's assigned consumer while keeping shared package and tool
  catalogs readable.
- [x] Version appliance definitions independently from credentials so service
  security upgrades run once, and reuse one locked Docker Cargo cache across
  server/job image rebuilds without writing build output into the workspace.
- [x] Make `vm doctor` report invalid project configuration and stale managed
  package access instead of declaring the installation ready.

Container tests cover fake Docker/provider execution, temporary controller
state, fixture source discovery, repeat reconciliation, targeted sidecar
updates, and volume-preserving command construction. Live Docker acceptance is
recorded below.

## Brainless Package Workflow

- [x] Select host and managed-guest command context explicitly in tests.
- [x] Quarantine unhealthy configured repositories without failing startup.
- [x] Report one package health state and apply only deterministic doctor fixes.
- [x] Add one-time project/source initialization and persistent work context.
- [x] Launch resumable managed work and infer release identity from its directory.
- [x] Activate published managed tools in configured project environments.
- [x] Keep one persistent Docker acceptance owner for the complete workflow.

The dedicated CI acceptance job uses a unique Compose project and host port,
builds local appliance images, initializes an isolated project, forces a
collection release through rework, verifies private
publication and automatic activation, then releases a two-target binary from
the canonical workspace into a second existing project. It compares every
stable Docker container ID and attached named volume before and after, and its
binary command proves that worker secrets are unreadable. The
current development container has no Docker binary, so live execution remains
on the Docker CI runner rather than being replaced with another fake-provider
test.

## Guest-Owned Package Work

This implemented package-infrastructure release slice supersedes the
host-launched `vm packages work` UX without replacing the durable checkout,
review, build, publication, or rollout engines already implemented.

### Product decision

The VM in which an agent is already running owns the editable checkout. The
package appliance is infrastructure only: it authenticates the request, serves
an immutable source bundle, records workflow state, reviews, integrates, builds,
publishes privately, and coordinates rollout. It must never start Codex, Claude,
Antigravity, or any other agent.

Normal agent flow inside a managed Docker or Linux Tart guest:

```bash
vm packages checkout typemill
# Continue work in the Source path printed by the command.
# Edit, test, bump the version when required, and commit.
vm packages release
```

When the guest workspace is already the registered canonical package source,
checkout is unnecessary:

```bash
# Edit, test, bump the version when required, and commit.
vm packages release
```

Consumers continue using native package-manager commands such as `npm install`
or `npm update`. Managed tools continue using the existing activation path. No
public npm, Cargo, PyPI, GitHub Release, or other external publication is added.

### Command contract

- [x] Make `vm packages checkout <source>` the only normal isolated-work entry
  point and allow it only from a managed guest.
- [x] Remove required `--agent`, `--consumer`, and `--task` arguments. Infer the
  consumer and actor from the guest's signed, consumer-bound capability. The
  agent's conversational task is not part of checkout identity.
- [x] Print one unambiguous `Source: <absolute-guest-path>` result plus a short
  `cd` hint. The CLI cannot change its parent process's directory; agents must
  use the printed path as the working directory for subsequent commands.
- [x] Keep checkout source beneath
  `$HOME/.local/share/vm/package-checkouts/<checkout-id>/source`, never beneath
  an infrastructure container's filesystem and never in `/workspace` unless
  `/workspace` is itself the registered canonical source.
- [x] Make checkout idempotent by current consumer and source. Resume the one
  nonterminal checkout, return its existing source path, and reject ambiguous
  duplicates instead of creating another checkout.
- [x] If durable workflow state exists but the guest copy is missing, reacquire
  a scoped lease and restore the checkout into the same managed path. Never
  silently replace a locally modified checkout.
- [x] Allow a scoped guest to check out any registered internal source it is
  authorized to read. Do not require the current project to already declare the
  package as a dependency.
- [x] Apply npm, Cargo, or Python development overrides only when the current
  project actually consumes that package. A source-only checkout must remain a
  valid workflow.
- [x] Keep bare `vm packages release` directory-inferred for both managed
  checkouts and attested canonical workspaces. Remove the checkout ID from the
  normal release syntax.
- [x] Make `vm packages cancel` infer the current managed checkout. Successful
  publication cleans up automatically. Retain ID-based show/cancel/cleanup only
  as hidden or clearly labeled controller diagnostics if operators still need
  them for repair.

The durable API may retain existing `agent` and `task` fields for receipt and
wire compatibility, but the guest CLI must not ask users to supply them. Populate
them from the authenticated actor and a stable internal guest-work purpose. Do
not make model choice part of idempotency or authorization.

### Remove the host agent launcher

- [x] Delete the public host agent-launch command.
- [x] Delete the package command's Codex executable, prompt construction, and
  interactive provider-launch path. Do not replace it with another agent
  launcher or an appliance daemon.
- [x] Stop persisting the retired package-work target keys.
  `vm packages init <source-root>` should configure the controller source shelf
  and appliance only; it must not select a project in which to launch work.
- [x] Tolerate old global configuration once and omit the retired keys on the
  next managed save. Do not retain permanent compatibility behavior that can
  still launch host-selected work.
- [x] Remove host-side checkout preparation, provider copy/exec adapters, and
  runtime-subject abstractions only after reference checks prove they have no
  remaining package callers. Preserve shared provider interaction used by
  unrelated VM commands.
- [x] Replace every obsolete package-work error hint with the exact
  guest command or canonical-workspace release instruction.

### Cruft cleanup rules

- [x] Delete code, tests, fixtures, schema fields, help text, and documentation
  whose only owner was the host agent launcher.
- [x] Consolidate duplicate host and guest checkout implementations around the
  guest path; do not leave two implementations behind feature flags or aliases.
- [x] Remove public `cleanup` mechanics that duplicate automatic successful
  cleanup or inferred `cancel`, while retaining the internal cleanup operation
  required for retries and administrator repair.
- [x] Keep `PackageExecutor` or similar abstractions only where at least two
  live runtime implementations remain after the host path is removed.
- [x] Update the package guide, CLI reference, configuration guide,
  troubleshooting guide, package-server README, changelog, and this tracker in
  the same change. No document may describe infrastructure as the agent owner.
- [x] Run reference checks for the retired launcher/config keys, required
  checkout agent/task flags, and ID-based normal release examples; every
  remaining result must be an intentional migration note or administrator-only
  diagnostic.

### Deterministic verification

- [x] CLI parsing accepts `vm packages checkout typemill` without flags and
  rejects host execution with one exact hint to run it inside a managed VM.
- [x] Capability tests prove the consumer and actor come from authenticated
  guest state and cannot be overridden from command-line input.
- [x] Checkout and release tests prove source-only package work reaches
  integration and release readiness without a fabricated consumer result,
  while an actual dependency still receives, validates, and later restores its
  development override.
- [x] Cancellation tests prove the consumer-bound agent cannot spoof cleanup,
  durable state remains cancelled until guest cleanup succeeds, and explicit
  closure is authenticated and idempotent.
- [x] Resume tests cover repeat checkout, guest restart, expired lease, missing
  local source, dirty local source, ambiguous durable state, and terminal prior
  checkouts.
- [x] Release tests prove bare release inference, rework, idempotent retry,
  automatic cleanup, immutable private publication, and unchanged canonical
  workspace state.
- [x] Security tests prove checkout paths cannot escape managed guest storage
  and that guests never receive controller Git credentials, builder tokens,
  publish credentials, or writable registry storage.
- [x] Static verification passes the repository gate below, including scoped
  warnings-as-errors Clippy and duplicate detection when `jscpd` is available.

### Live Docker acceptance contract

`scripts/internal/test-package-workflow-docker.sh` is the sole acceptance
owner; do not add a second script. It must:

1. Start one isolated package appliance and two existing project containers.
2. Invoke `vm packages checkout <source>` through the guest client inside the
   producer container, with no host `work` command and no agent flags.
3. Assert the checkout exists under the producer guest's managed home and does
   not exist in gateway, workflow, reviewer, builder, releaser, or rollout
   container filesystems.
4. Restart the producer and prove repeat checkout resumes the same durable
   checkout and source path without losing committed work.
5. Make, test, version, and commit a fixture change inside the guest checkout;
   run bare `vm packages release` there and exercise one deterministic rework
   cycle.
6. Prove no appliance process launches an AI agent and no guest process receives
   controller, build, or publish credentials.
7. Install or activate the immutable result in the second existing container
   and execute the released version.
8. Repeat checkout/release/update operations and prove they are no-ops where
   appropriate.
9. Compare primary container IDs and persistent volume identities before and
   after. No project environment or appliance data volume may be recreated to
   perform package work.
10. Clean only the acceptance test's unique containers, sidecars, networks,
    volumes, images, and temporary paths.

The existing acceptance script now drives this guest-owned flow directly,
removes its fake agent launcher, downloads review input as immutable bundles,
and checks restart/resume, rework, activation, credentials, container IDs, and
volume identity. The current development container has no Docker binary, so the
revised live suite still requires its Docker runner rerun.

### Completion criteria

This slice is complete only when an agent already running in a managed VM can
check out an arbitrary authorized registered source, survive a VM restart, edit
and commit it, run bare release, and make the immutable private result available
to another existing VM without any host-launched agent, checkout ID, model flag,
environment rebuild, public publication, or infrastructure credential exposure.

Do not expand this slice into a new package daemon, release engine, agent
orchestrator, registry protocol, public publishing workflow, or transactional
`vm.yaml` apply operation.

## Canonical Workspace and Binary Tool Releases

- [x] Extend the existing source, manifest, and release records for attested
  canonical workspaces, durable internal source archives, and binary tools.
- [x] Detect and resume canonical-workspace releases without repository-local
  state, checkout identifiers, or mutations.
- [x] Build binary artifacts in a dedicated no-egress worker under an
  unprivileged UID, with a narrow build credential inaccessible to repository
  commands; let the credentialed releaser publish only durable staged bytes.
- [x] Mount the Docker builder token beneath a root-owned `0700` directory
  instead of relying on unsupported Compose secret modes; live Docker checks
  prove the post-`setuid` repository process cannot read either credential path.
- [x] Review the complete tree for a first internal workspace release, then use
  the last internally published source commit across every later local commit.
- [x] Return deterministic binary build failures to receipted rework, retain
  infrastructure failures for retry, and reuse the same staged bytes after a
  publisher restart.
- [x] Reuse target-aware managed-tool installation and activation for explicitly
  configured environments.
- [x] Prove workspace release, rework, private publication, activation, and
  Docker persistence in the real acceptance suite.

The implementation must remain an adapter over checkout, submission, review,
integration, publication, and activation services. It must not introduce a
second release engine or tool installer, and fixtures must remain generic rather
than encode a particular downstream tool.

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

The 2026-08-16 container gate passed formatting, workspace checking, unit and
integration tests, warnings-as-errors Clippy, and `git diff --check`. The
duplicate script was invoked but `jscpd` is not installed in this container, so
that conditional check remains assigned to the equipped CI runner. One
parallel provider assertion failed once, then passed focused, with its feature
set, and on the full unit rerun without a code change.

Focused live Docker acceptance on 2026-08-15 used a unique appliance, producer,
consumer, port, networks, and volumes. It proved deterministic build failure
returns to rework, a corrected canonical workspace publishes two immutable
binary targets without source mutation, the build command cannot read either
credential path, and a second running environment installs and executes the
private Linux ARM64 artifact as version 1.0.0. The isolated environments,
sidecars, volumes, networks, images, and temporary files were then removed.

The same change passed the serial all-feature workspace check, the full unit
suite, 59 focused package tests, package CLI parsing, package reconciliation,
workflow integration, formatting, Compose and shell syntax, and scoped
warnings-as-errors Clippy. The unrelated `temp_workflow_tests` target still
invokes the removed `vm temp` command and remains outside this release tracker.

Latest result on 2026-08-13: formatting, the serial all-feature workspace check,
all-target workspace Clippy with warnings denied, 500 workspace library tests,
the 165-test VM command suite, and the 160-test integration feature matrix
passed; 20
tests requiring a real runtime or performance environment remained ignored.
Focused coverage includes consumer-bound agent capabilities, retryable package
workflow state, persistent worker queues, private-only publication, automatic
consumer rollout creation, source discovery, writable-release replacement, and
repeat reconciliation. `git
diff --check` passed. Duplicate detection could not run because `jscpd` is not
installed in this container. No Docker, Tart, host VM, external network, or
publication action was run.

The focused package-boundary and consolidation audits on 2026-08-13 found no
dependency cycles, cross-crate source imports, misplaced workspace members, or
competing documentation/task owners. The obsolete standalone package-server
manual and component changelog were removed in favor of the canonical package
infrastructure guide and root changelog. Legacy project-local agent prompts and
the outdated Docker-in-Docker note were removed because the managed
`agent-skills` collection and testing guide own those workflows. Package
identity normalization now has one domain owner, release publishers share one
credential-aware Git command owner, and repeated workflow fixtures are
consolidated without reducing coverage. The workflow store now owns only
persistence and commits; checkout, receipt, catalog, consumer, and rollout state
each live with their canonical domain owner. Shared Git execution and
managed-path enforcement remain in the source manager, while worktree,
submission, integration, and rollout source lifecycles are isolated behind
their existing workflow API. Configuration validation now borrows one
configuration through a single ordered pipeline, with pure project, network,
runtime, and storage checks separated from mutable host checks.
Formatting, focused package/CLI tests, scoped all-target Clippy with warnings
denied, and `git diff --check` passed. Rollout reconciliation remains an
explicit authenticated `POST` command with no obsolete compatibility path.

## Post-Recreation Acceptance

The first Docker shell smoke test exposed and fixed an uninitialized zsh prompt
hook, then revealed that standard AI CLIs had been incorrectly coupled to the
private package appliance. Vibe bases again own Antigravity, Claude Code, and
Codex; only `agent-skills` remains selected through `vm tools`. Tart appliance
startup now accepts the inventory format returned by Tart 2.32.1. Full package
appliance acceptance remains tracked in
[Remaining Acceptance](#remaining-acceptance).

Docker appliance startup now succeeds with the source-installed CLI, remains
healthy on a no-flags restart from outside the project, and retains its named
volumes. The rebuilt Docker Vibe base now creates `vm-dev` while reusing its
preserved PostgreSQL service, `yocodex` resolves Codex 0.147.0 outside synced
state, and `agent-skills` 0.6.1 activates 26 skills across all five supported
agent locations without replacing Codex system skills. The flat source shelf
registered 13 npm packages while routing `agent-skills` to `vm tools`. These
provide the earlier live baseline summarized below. The current outstanding
checks are listed only in [Remaining Acceptance](#remaining-acceptance).

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

Completed live evidence includes appliance restart outside a project,
`agent-skills` 0.6.1 bootstrap, authenticated guest-client repair, Node/Cargo
and Git identity reconciliation, lease recovery, deterministic rework,
generation-scoped validation, and the guest-owned `agent-skills` 0.8.0 release
and activation in `projects-dev`. That release preserved the project container
identity and closed its durable checkout. Outstanding host, Docker, and Tart
checks are tracked once in [Remaining Acceptance](#remaining-acceptance).

## Related Documentation

- [Package Infrastructure](../docs/user-guide/package-infrastructure.md)
- [Configuration](../docs/user-guide/configuration.md)
- [Troubleshooting](../docs/user-guide/troubleshooting.md)
- [Development Architecture](../docs/development/architecture.md)
- [Testing](../docs/development/testing.md)

## Layman's Result

- Workers use one predictable local package service and keep useful read-only
  cache when the central appliance is briefly unavailable.
- Package experiments are isolated to the requesting guest and clean up their
  own checkout without touching canonical source repositories or Git history.
- An agent already running in a managed guest can release through the private
  appliance without a host launcher; configured tools activate in place and
  language consumers receive tested upgrade branches.
- Tart discovery, base acquisition, shell recovery, and mounts follow bounded,
  storage-aware paths.
- Cleanup, secrets, database services, snapshots, and self-updates are narrower
  and safer than before.
