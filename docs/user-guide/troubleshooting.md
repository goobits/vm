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

## Docker In A macOS Tart Guest

Tart does not support nested virtualization for macOS guests. Docker inside a macOS Tart guest uses Colima with QEMU TCG software emulation.

After booting the guest:

```bash
/workspace/start-colima
docker version
docker run --rm busybox echo run-ok
docker buildx version
docker compose version
```

If nested virtualization is unavailable, use a remote Docker daemon over SSH/TLS. Do not expose an unauthenticated Docker socket.

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
vm system registry status
vm system registry ls
```

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
