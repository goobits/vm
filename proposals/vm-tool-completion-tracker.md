---
Status: Rolling
Date: 2026-08-11
Depends: docs/user-guide/package-infrastructure.md, docs/development/architecture.md
---

# VM Tool Completion Tracker

This is the single remaining-work tracker for the active VM-tool release. It covers the worker-local package edge, package checkout lifecycle, Tart reliability, provisioning regressions, and previously reported security findings. Package development and release work must run in Docker or Linux VMs, not as native Mac workloads.

## Current Verdict

The shared package appliance, deterministic resolver, immutable releases, isolated source checkouts, explicit rollouts, portable mounts, and nonblocking tool updates are implemented. The current working tree adds a worker-local, read-only package edge with persistent cache and Docker or Linux Tart wiring. That slice was checked before the machine became unstable, but it still needs a final source audit and a fresh `cargo check` before commit.

The remaining risk is concentrated in durable development overrides, Tart transport and storage consistency, provisioning regressions, resource pressure, and a small set of unresolved security findings. Live Docker and Tart verification is intentionally deferred until the development environment is recreated.

## Non-Negotiable Boundaries

- Never stage or rewrite the user's `vm.yaml` as part of implementation commits.
- Never remove `/workspace`, a registered package source, its `.git` data, or `/data/sources`.
- Cleanup may remove only validated managed checkout, integration, rollout, cache, or staging paths.
- Project and integration agents never receive host Git credentials or writable registry storage.
- A published package coordinate must always return identical bytes.
- Internal dependencies fail closed. Public fallback is allowed only for packages classified as external.
- Startup must not wait on update checks or an unavailable package appliance.
- Keep Tart at 2.32.1 until the host supports a newer version without the Swift runtime crash.

## Recovery Checkpoint

The implementation checkpoint begins after commit `cefc06a4` (`feat(packages): centralize source resolution policy`). The dirty package-edge slice spans `vm-config`, `vm-package-server`, `vm-package-work`, `vm-packages`, Docker Compose rendering, Tart package provisioning, and the `vm packages` runtime. Inspect `git status --short` before resuming because `vm.yaml` is user-owned and intentionally dirty.

Previously verified before the no-build pause:

- `vm-package-server`: 75 tests passed.
- `vm-packages`: 18 tests passed.
- Docker Compose rendering tests passed.
- Tart package-provisioning tests passed.
- VM package-runtime tests passed.
- Strict Clippy passed for `vm-packages`, `vm-package-server`, `vm-provider`, and `goobits-vm`.

These results are evidence for the checkpoint, not a substitute for a fresh check after the final edits.

## Remaining Tasks In Order

### Phase 1: Finish the worker-local package edge

LOC: +50-100 / -20-40

Verify now: Rust formatting, focused package-server tests, and `cargo check` for the changed package crates. Do not start Docker or Tart.

- [ ] Finish the HTTP-level cache and infra-restart test for internal npm metadata and artifacts.
- [ ] Make catalog refresh nonblocking, bounded, and quiet during a sustained outage. Log state transitions instead of warning every interval.
- [ ] Confirm the read-only edge exposes no publish or tool-write routes and receives no publish credentials.
- [ ] Confirm internal and public caches cannot satisfy each other's package classifications.
- [ ] Confirm Docker restart can add the edge sidecar without rebuilding the project image.
- [ ] Confirm Linux Tart provisions the same edge contract through its nested Docker runtime.
- [ ] Commit the package-edge slice without `vm.yaml`.

Likely files:

```text
~ rust/vm-package-server/src/{server,resolver,storage,internal}.rs
~ rust/vm-package-server/src/{npm,pypi}.rs
~ rust/vm-package-server/src/cargo/*
~ rust/vm-package-work/src/{server,store}.rs
~ rust/vm-packages/src/{client,environment}.rs
~ rust/vm-provider/src/docker/{compose.rs,template.yml}
~ rust/vm-provider/src/tart/provisioner/packages.rs
~ rust/vm/src/commands/packages/{appliance,runtime}.rs
```

### Phase 2: Make development overrides durable and reversible

LOC: +180-280 / -80-160

Verify now: adapter unit tests and `cargo check -p goobits-vm`. Live package-client checks wait for the recreated environment.

- [ ] Give every assigned checkout one managed override record with checkout ID, consumer, ecosystem, source, and restoration data.
- [ ] Make npm use an explicit temporary local source without permanently changing the manifest or lockfile.
- [ ] Make Cargo use a persistent, checkout-scoped patch configuration instead of a one-shot `cargo metadata` command.
- [ ] Make Python use an isolated editable install with enough recorded state to restore the pinned release.
- [ ] Refuse silent fallback when an assigned worktree is missing.
- [ ] Restore the exact published dependency before deleting local checkout data.
- [ ] Centralize managed-path validation and prove cleanup cannot reach the canonical mirror, source repository, `.git`, or workspace.
- [ ] Remove agent and integration worktrees after successful integration or release while retaining only required immutable bundles and receipts.

Likely files:

```text
~ rust/vm/src/commands/packages/checkout.rs
~ rust/vm/src/commands/packages/{mod,release,runtime}.rs
~ rust/vm-package-work/src/{source,server,store}.rs
~ rust/vm-package-work/src/source_tests.rs
```

### Phase 3: Normalize Tart command, storage, discovery, and base handling

LOC: +180-300 / -80-160

Verify now: Tart feature unit tests and `cargo check -p vm-provider --features tart`. Do not invoke the Tart binary.

- [ ] Route all Tart invocations through one command context that consistently applies configured storage.
- [ ] Persist or recover the storage context used to create a managed VM so `vm list`, `vm ssh`, lifecycle commands, and package infrastructure see the same instances.
- [ ] Detect a running managed Tart process using a nondefault storage location without scanning unrelated disks.
- [ ] Make base acquisition transactional. Pull or build into a uniquely named staging VM, validate it, rename it into place, and remove only managed staging data on failure.
- [ ] Keep a usable known-good base until its replacement succeeds.
- [ ] Report GHCR authorization failure clearly before the bounded local fallback.
- [ ] Keep Tart 2.32.1 as the supported host version for this machine.

Likely files:

```text
~ rust/vm-provider/src/tart/{command,instance,provider}.rs
~ rust/vm-provider/src/tart_base.rs
~ rust/vm/src/commands/{base,packages/tart}.rs
~ rust/vm/src/commands/vm_ops/{list,targets}.rs
```

### Phase 4: Finish Tart shell identity, mount recovery, and responsiveness

LOC: +250-400 / -80-180

Verify now: shell argument, key-management, readiness, mount-command, and deadline tests plus Rust checks. Two live passwordless connections and real mount recovery wait for the recreated environment.

- [ ] Generate one controller key at `~/.vm/ssh/tart_ed25519` with restrictive permissions.
- [ ] Install its public key into the guest during provisioning without replacing existing authorized keys.
- [ ] Permit one explicit password bootstrap for an existing VM, then require the managed key on subsequent direct SSH connections.
- [ ] Add `IdentitiesOnly=yes`, the managed identity file, short connection timeouts, keepalive settings, and an ephemeral host-key policy to native SSH.
- [ ] Keep `vm exec` strict on the guest agent while allowing `vm ssh` to use the verified SSH transport.
- [ ] Use one real readiness deadline and avoid duplicate guest-agent probes.
- [ ] Detect and remount missing configured VirtioFS shares before shell or command use without recreating the VM.
- [ ] Preserve configured read-only or read-write permissions when remounting.
- [ ] Add resource-pressure warnings when concurrent Tart and Docker Desktop allocation envelopes materially exceed host capacity. Never silently rewrite explicit limits.

Likely files:

```text
~ rust/vm-provider/src/lib.rs
~ rust/vm-provider/src/tart/{readiness,shell,mounts,provisioner}.rs
~ rust/vm/src/commands/vm_ops/{lifecycle,interaction}.rs
~ rust/vm/src/commands/vm_ops/tests/lifecycle.rs
~ rust/vm-core/src/user_paths.rs
```

### Phase 5: Fix provisioning, tooling, and descriptor-pressure regressions

LOC: +120-220 / -60-130

Verify now: Ansible syntax parsing, source-level scenario tests, focused Rust tests, and `cargo check`. Real provisioning waits for the recreated environment.

- [ ] Replace the duplicated AI-sync Jinja expressions with one valid derived policy and fix the antigravity parenthesis regression.
- [ ] Cover boolean and granular `host_sync.ai_tools` forms for Claude, Codex, and Antigravity, including the deprecated Gemini compatibility key.
- [ ] Confirm configured tool collections and command shims are installed on a newly provisioned guest without blocking startup on update checks.
- [ ] Audit every long-lived Tart and streamed-command child for closed stdin, bounded output ownership, waits, and cleanup.
- [ ] Replace name-only temporary-container cleanup with an explicit VM-owned label.
- [ ] Ensure command output remains pipe-safe and broken pipes never panic the CLI.
- [ ] Add a troubleshooting diagnostic for host file-descriptor pressure and resource oversubscription.

Likely files:

```text
~ rust/vm-provider/src/resources/ansible/playbook.yml
~ rust/vm-provider/src/tart/*
~ rust/vm-core/src/command_stream.rs
~ rust/vm/src/commands/{clean,doctor}.rs
~ docs/user-guide/troubleshooting.md
```

### Phase 6: Close the unresolved security findings

LOC: +300-550 / -100-250

Verify now: focused path, permissions, transaction, SQL, cleanup, and checksum tests plus workspace Rust checks. Runtime-specific checks wait for the recreated environment.

- [ ] Re-audit snapshot import and restore path components and reject every absolute, parent, symlink, hardlink, and project-adjacent escape.
- [ ] Keep existing snapshots until staged replacement fully validates and installs.
- [ ] Re-audit package-server bind, auth, decoded route, immutable publication, and filesystem-path boundaries after the edge changes.
- [ ] Re-audit Tart host command construction so configuration values never enter an unquoted shell command.
- [ ] Retain plugin-name validation and transactional database restore with quoted identifiers and literals.
- [ ] Create secret directories and files with restrictive permissions, and repair older overly broad permissions when safely opened.
- [ ] Scope cleanup to VM-owned labels and named resources. Never perform global image or build-cache pruning as an incidental cleanup.
- [ ] Keep test fixtures in unique temporary directories.
- [ ] Verify release SHA-256 assets before replacing the installed CLI binary.
- [ ] Add regression tests for each formerly reported finding and remove superseded duplicate helpers.

Likely files:

```text
~ rust/vm-snapshot/src/{create,import,manager,restore}.rs
~ rust/vm-package-server/src/{server,validation,storage}.rs
~ rust/vm-provider/src/tart/*
~ rust/vm/src/commands/{plugin,clean,update}.rs
~ rust/vm/src/commands/db/backup.rs
~ rust/vm-core/src/secrets.rs
~ rust/vm-config/tests/config_ops_tests.rs
```

### Phase 7: Documentation and post-recreation acceptance

LOC: +100-180 / -50-100

Verify now: links, command names, formatting, complete `cargo check`, and the repository's Rust test suite that does not start Docker or Tart.

- [ ] Update the package-infrastructure guide to show the worker edge, offline behavior, explicit internal failure, and restart versus rebuild rules.
- [ ] Document the canonical Tart storage context, SSH bootstrap, resource guidance, and mount recovery behavior.
- [ ] Reconcile architecture docs and remove stale descriptions or duplicate plans.
- [ ] After recreation, run the package appliance as Docker and start a separate Docker worker on the same managed network.
- [ ] Prove npm, Cargo, and Python public proxying, immutable internal artifacts, per-worker override isolation, restart recovery, and clear internal failure when uncached.
- [ ] Prove two consecutive `vm ssh` sessions to the same Tart guest. The second must be passwordless.
- [ ] Prove a missing VirtioFS workspace is remounted without source loss.
- [ ] Run the full repository build, lint, test, and container matrix only after the recreated machine is stable.

Likely files:

```text
~ docs/user-guide/package-infrastructure.md
~ docs/user-guide/{configuration,troubleshooting,cli-reference}.md
~ docs/development/{architecture,testing}.md
~ proposals/vm-tool-completion-tracker.md
```

Total LOC: +1,180-2,030 / -470-1,020

## Completed Foundations

- [x] Shared package appliance with private gateway, durable volumes, workflow state, and receipts.
- [x] Authenticated immutable npm, Cargo, Python, OCI-cache, and tool artifact paths.
- [x] Isolated concurrent package checkouts and serialized integration.
- [x] Explicit consumer inventory, drift, rollout, backup, and recovery workflows.
- [x] One canonical package identity and resolver policy shared by ecosystem adapters.
- [x] Portable multiple mounts with read-only and read-write permissions.
- [x] Nonblocking guest-tool update prompts and immutable tool collections.
- [x] `vm ssh` can create a missing environment and has an initial Tart SSH fallback.
- [x] Pipe-safe configuration output.

## Verification Log

- 2026-08-11 Package-edge checkpoint had focused unit tests and strict Clippy green before the final HTTP restart test and later edits.
- 2026-08-11 Live Tart investigation found a nondefault storage context and a missing VirtioFS workspace share. The host source remained intact and the share was remounted manually.
- 2026-08-11 Live host evidence showed Tart and Docker Desktop allocation envelopes could exceed physical memory under simultaneous load.

## Related Context

- [Package Infrastructure](../docs/user-guide/package-infrastructure.md): current user-facing architecture and commands.
- [Development Architecture](../docs/development/architecture.md): repository component ownership.
- [Troubleshooting](../docs/user-guide/troubleshooting.md): operational recovery guidance.

## Layman's Wins

- Every development box gets fast, stable package URLs without making the shared infra service a startup bottleneck.
- Package experiments stay isolated to one worker and clean themselves up without risking real source code or Git history.
- Tart follows one predictable command and storage path, reconnects safely, and recovers missing mounts.
- Security-sensitive replacement, restore, cleanup, and update operations become staged, scoped, and retry-safe.
