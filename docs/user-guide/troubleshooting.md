# Troubleshooting

Start with diagnostics:

```bash
vm doctor
vm doctor --fix
```

## Environment Will Not Start

```bash
vm list
vm logs dev --tail 100
vm stop dev
vm run linux as dev
```

For a clean active environment rebuild:

```bash
vm config render
vm remove dev --force
vm run linux as dev
```

Review the rendered mounts before removal. Saved snapshots and stable
keep-retention volumes are preserved, but files that exist only in a
container's writable layer are disposable.

## Missing package.json

Bootstrap installs dependencies only when the configured workspace contains
both `package.json` and a supported lockfile. An `ENOENT` for
`/workspace/package.json` means an older bootstrap path ran a package manager
without first detecting the project files, or `/workspace` points at the wrong
host directory.

```bash
vm config render
vm config validate
```

Confirm the rendered `/workspace` bind before applying lifecycle changes.

## Cannot Open A Shell

```bash
vm list
vm ssh
vm exec -- pwd
```

`vm ssh` creates the selected environment from `vm.yaml` when it is missing and
starts it when stopped. `vm exec` starts an existing stopped environment but
does not create one.

For Tart, `vm ssh` prefers the guest agent. A running macOS guest can fall back
to native SSH. The first recovery of an older guest may ask for the `admin`
password while installing `~/.vm/ssh/tart_ed25519.pub`; repeat connections must
be passwordless. If both transports fail, run `vm doctor` and inspect the Tart
run log named in the error.

`tart list` shows every local Tart VM; `vm list` intentionally shows only
VM-managed instances recorded in `~/.vm/tart/instances.json`. If an expected
managed instance is missing, also check whether it uses a nondefault storage
volume:

```bash
echo "${TART_HOME:-<default>}"
vm doctor
tart list --format json
```

Managed instances now retain their creation storage in
`~/.vm/tart/instances.json`. Do not move or delete the underlying Tart VM
directory while it is running.

## Docker In Tart

The `vibe-tart` default is a Linux guest. Docker Engine runs directly inside
that guest:

```bash
vm config profile set tart
vm ssh
docker version
docker run --rm busybox echo run-ok
```

If the standard Linux base is missing, environment creation first tries the
published versioned base and then builds it locally. Run `vm doctor` for Tart
dependency diagnostics, or deliberately rebuild with:

```bash
vm system base build vibe --provider tart
```

The separate macOS profile is the slower fallback for macOS-only tools.

Tart does not support nested virtualization for macOS guests. Docker inside a
macOS Tart guest uses Colima with QEMU TCG software emulation.

After booting the guest:

```bash
/workspace/start-colima
docker version
docker run --rm busybox echo run-ok
docker buildx version
docker compose version
```

For faster Docker, switch back to the Linux Tart profile or use a controlled
remote Docker daemon over SSH/TLS. Do not expose an unauthenticated Docker
socket.

## Port Conflicts

```bash
vm config ports --fix
vm tunnel ls
vm tunnel stop 8080
vm tunnel add 8080:3000 dev
```

Named instances can coexist, but complete stacks cannot share the same host
ports. Assign distinct ports before starting simultaneous stacks.

## Runtime And Storage

Target one container environment to collect read-only runtime evidence:

```bash
vm status container
vm status dev
```

The report separates writable-layer bytes from named-volume usage and includes
`/tmp`, memory/PID peaks and limits, log rotation, and stop policy. A bare
`vm status` lists environments without scanning their storage.

Use pnpm store pruning only when measured growth warrants it and no install is
using a shared store:

```bash
vm doctor --prune-pnpm-store --container dev
```

Pruning is never part of startup.

If macOS reports `too many open files in system`, stop spawning new providers
and run `vm doctor`. It reports host descriptor use and warns at 85%. Stop stale
VM, container, browser, or helper processes before retrying. Timed VM commands
now terminate and reap their child process instead of leaving output threads and
pipes behind.

## Safe Recreation

Before recreating an older environment, inventory credentials, documents,
database data, shell history, and other state outside declared mounts. Render
the candidate config first and verify stable physical volume names.

Do not use `down -v`, volume/system prune, or copy old writable-layer caches
into new volumes. Do not persist process sockets or replay terminal panes.
Rollback should restore the previous config and recreate against the preserved
source binds.

## Package Registry

```bash
vm packages status
vm packages doctor --fix
```

`status` prints one classification: `healthy`, `degraded`, or `action required`.
`doctor --fix` repairs only deterministic state and prints one repair command
for anything that still needs an operator. For managed tools it also restarts a
missing activation worker, repairs stale package sidecars and trusted-source
registration drift, requeues interrupted activations, and resumes pending
executable-adoption receipts through normal reconciliation.

The central appliance and each worker edge have separate failure behavior. If
the appliance is down, a warmed edge can serve cached locked internal artifacts
and proxy known-external packages publicly. Uncached internal packages fail
closed. If the worker edge itself is down, restart the project environment;
package clients intentionally do not bypass it because doing so could leak an
internal name to a public registry.

`vm packages up` validates configured source roots before starting or updating
the appliance. A missing absolute root still stops before service
reconciliation. An unhealthy child Git repository is moved intact under
`.vm-quarantine`, healthy siblings continue, and the command reports
`degraded`. Repair it with `vm packages doctor --fix`; an existing empty shelf
remains valid.

If archived repositories reuse a current package name with a different Git
origin, put an empty `.vm-packages-ignore` file at the archived subtree root and
rerun `vm packages up`. The [Package Infrastructure guide](package-infrastructure.md#advanced-initialize-package-work)
owns the full equivalent-clone and conflicting-origin rules.

Canonical project workspaces use a different, read-only policy. If bare `vm
packages release` reports missing attestation, first run `vm packages doctor
--fix` on the host to repair registration drift. If the physical repository is
not yet a trusted canonical source, enroll it with `vm packages register
<local-path>`, then reconcile the environment. Another clone with the same
origin is intentionally rejected. Missing paths, invalid manifests, and origin
mismatches report degraded health but are never moved or repaired.

Release also requires a clean committed worktree. Commit or discard local and
untracked changes yourself, and correct an origin mismatch explicitly before
retrying. VM never resets files, changes remotes, creates tags, or repairs an
exact canonical source.

If release remains in `ready_to_release`, inspect `vm tools status
[environment]` for its durable job ID and workflow. The release command prints
the same ID, reports durable build and activation phase changes, and emits a
heartbeat every 10 seconds.
`Ctrl-C` only detaches; rerun `vm packages release` to resume, or run `vm
packages cancel` from a managed checkout to cancel explicitly. A full builder
temporary filesystem is reported as an unhealthy package service. Reconcile it
with `vm packages up`; the builder starts by removing only stale directories in
its dedicated managed work root, without changing workflow records, artifacts,
project environments, or named volumes.

An isolated binary-build launcher or worker I/O failure is infrastructure, not
source rework. The job stays queued with retry backoff and logs the failing
stage, manifest program, and managed working directory. Repair the appliance
with `vm packages doctor --fix` or `vm packages up`, then rerun `vm packages
release`; the approved immutable integration resumes without reading or changing
newer worktree edits. A build command that runs and exits unsuccessfully still
requests source changes normally.

An older VM CLI never rewrites a package appliance with a newer definition
revision. It stops before materializing Compose files and tells you to run `vm
update`; this prevents host/controller version skew from downgrading a healthy
worker configuration.

For an existing environment, run this on the controller host:

```bash
vm tools status [environment]
vm tools update --to <environment>
```

`PROJECT_COPY=yes` means a standalone project checkout shadows the managed
collection. Remove it when VM should own the collection, or update it through
Git and disable the managed tool when the repository should own it. VM never
rewrites project Git.

`update` repairs the worker edge, VM-owned vendor tools, and managed package
links without a base rebuild or persistent edge-cache removal. Select a vendor
explicitly to fetch its latest official release:

```bash
vm tools update codex --to <environment>
vm tools update claude antigravity --to <environment>
```

If VM refuses to overwrite an unmanaged launcher, resolve that ownership before
retrying; it never silently replaces unrelated executables. A background shell
repair may finish after the terminal opens; inspect its guest log with the
vendor name:

```bash
tail -n 50 "${XDG_STATE_HOME:-$HOME/.local/state}/vm-runtime/<vendor>.log"
```

If an older environment prints zsh job lines such as `[5] 26237` around a
`git worktree repair` command, reinstall the current host CLI and connect once.
The detached runtime repair removes that obsolete per-shell hook; worktree
repair remains targeted to a broken linked worktree and stays silent. Shell
attachment itself does not wait for package or tool maintenance.

For a deterministic foreground result, run `vm tools refresh` followed by `vm
tools update --to <environment>` on the host. The update waits for an in-flight
repair and fails if a required vendor or managed package tool remains unusable.

If a package/tool command was run inside a managed guest, do not try to operate
the controller from there. The error prints the exact shell-safe host command,
such as `Run on the host: vm packages up`; run that command in the host terminal.

A built-in tool can be registered but not published on a fresh controller. From
an existing managed guest, run `vm packages checkout agent-skills`, continue in
the printed source path, commit the intended versioned change, and run bare `vm
packages release`. A globally enabled collection activates automatically across
running environments; use `vm tools update --to <environment>` only for targeted
repair. See [Package Infrastructure](package-infrastructure.md#advanced-tool-manifests-and-targeting)
for registration, targeting, locking, and package-state behavior.

If `vm packages cancel` reports that dependency restoration failed, do not
delete the checkout directory manually. Fix the reported local package-manager
or permission problem and rerun the same command from the checkout source. The
durable checkout remains cancelled until local restoration succeeds.

## Secrets

```bash
vm secret status
vm secret ls
```

## State

```bash
vm save dev as before-change
vm revert dev before-change
vm package dev --output dev.tar.gz
```

## Debug Output

```bash
LOG_LEVEL=debug LOG_OUTPUT=console LOG_FORMAT=human vm run linux as dev
VM_DEBUG=true vm run linux as dev
VM_VERBOSE=true vm run linux as dev
```

CLI logs default to a file so requested command output stays clean. Long-running
services default to JSON on stderr and include an `x-request-id` on HTTP
responses for correlation.
