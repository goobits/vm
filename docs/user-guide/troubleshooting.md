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

If `tart list` and `vm list` disagree, check whether the VM uses a nondefault
storage volume:

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
vm packages doctor
```

The central appliance and each worker edge have separate failure behavior. If
the appliance is down, a warmed edge can serve cached locked internal artifacts
and proxy known-external packages publicly. Uncached internal packages fail
closed. If the worker edge itself is down, restart the project environment;
package clients intentionally do not bypass it because doing so could leak an
internal name to a public registry.

`vm packages up` validates configured source roots before starting or updating
the appliance. Fix a reported missing/invalid absolute root and retry; no
service reconciliation has occurred. An existing empty configured shelf is
valid and will be scanned again on the next run.

For an existing environment, run this on the controller host:

```bash
vm tools status [environment]
vm tools update [environment]
vm tools update --fleet [--provider docker] [--pattern 'project-*']
```

`status` distinguishes registered, published, installed, and consumable tools,
including registrations or stale guest installs no longer selected by the
project. `PROJECT_COPY=yes` means the project also contains a standalone Git
checkout at one of that collection's activation paths. That checkout is not a
failed sync: project Git is intentionally never mutated by `vm tools`. Remove
the checkout when VM should own the collection, or update it separately and
disable the overlapping managed tool when the repository should own it.
`update` repairs a missing/stale package edge, incomplete standalone Codex
package, and broken managed-tool links. Docker updates only the sidecar; Linux
Tart updates only its edge container. Neither path rebuilds the base or removes
the persistent edge cache volume. Codex replacement is transactional and
refuses to overwrite an unmanaged `/usr/local/bin/codex`; inspect and resolve
that ownership explicitly before retrying. Interactive shell startup launches
this Codex probe/repair in the background, so a broken legacy environment can
open before `yocodex` becomes usable. Inside that guest, inspect the append-only
job log with:

```bash
tail -n 50 "${XDG_STATE_HOME:-$HOME/.local/state}/vm-runtime/codex.log"
```

For a foreground result, run `vm tools update [environment]` on the host.
It waits for any repair already in flight and returns an error if Codex is still
not consumable.

Opening several terminals at once should produce only one active catalog,
Codex, or managed-tool reconciliation job for that environment. Successful
shell-triggered work is reused for 60 seconds. If a deterministic immediate
check is needed, run `vm tools refresh` followed by `vm tools update
[environment]` on the host; explicit commands are not delayed by that
shell cooldown.

If a package/tool command was run inside a managed guest, do not try to operate
the controller from there. The error prints the exact shell-safe host command,
such as `Run on the host: vm packages up`; run that command in the host terminal.

A built-in tool can be registered but not published on a fresh controller. That
is intentional. Run the reported explicit command, normally
`vm tools publish agent-skills`, then rerun `vm tools update`. The first update
still reconciles the worker edge and Codex before reporting the unpublished
collection.

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
LOG_LEVEL=DEBUG vm run linux as dev
VM_DEBUG=true vm run linux as dev
VM_VERBOSE=true vm run linux as dev
```
