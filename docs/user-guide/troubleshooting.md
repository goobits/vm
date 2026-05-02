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
vm remove dev --force
vm run linux as dev
```

Saved snapshots are preserved by `vm remove`.

## Cannot Open A Shell

```bash
vm list
vm shell dev
vm exec dev -- pwd
```

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
