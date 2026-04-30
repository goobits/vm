# Configuration

`vm` works from intent-first commands and uses `vm.yaml` for durable project defaults.

```bash
vm run linux as backend
vm run mac as xcode
vm run container as redis
```

A minimal config:

```yaml
version: '2.0'
provider: docker
project:
  name: backend
  workspace_path: /workspace
vm:
  image: ubuntu:24.04
  memory: 8192
  cpus: 4
ports:
  _range: [3000, 3099]
```

## Managing Config

```bash
vm config validate
vm config show
vm config get vm.memory
vm config set vm.memory 8192
vm config unset vm.swappiness
vm config ports --fix
vm config clear
```

Profiles remain available for project variants:

```bash
vm config profile ls
vm config profile set docker
vm run linux as backend --profile docker
```

## Provider Routing

Daily commands use environment kinds. Provider names are advanced routing overrides.

```bash
vm run linux as backend
vm run linux as isolated --provider tart
vm run container as db --provider podman
```

## macOS Tart Guests With Docker

For Docker inside a macOS Tart guest, the Mac runner must be Apple Silicon M3/M4 on macOS 15+ and the guest image must be macOS 15+. Enable nested virtualization in the Tart config:

```yaml
tart:
  guest_os: macos
  nested: true
  install_docker: true
```

`vm run mac` then launches Tart with `--nested` and installs Docker CLI, Compose, Buildx, Colima, and QEMU inside the guest. Start Docker in the guest with:

```bash
/workspace/start-colima
docker run --rm busybox echo run-ok
```

## Presets

```bash
vm config preset --list
vm config preset nodejs
vm config preset python,postgres
```

## Package Registry And Secrets

System plumbing lives under `system`; plugin-backed user workflows stay flat.

```bash
vm system registry status
vm system registry ls
vm secret status
vm secret ls
```

## Worktrees And Workspace Paths

Open a shell with:

```bash
vm shell backend
```

Inside the environment, project files are mounted at `project.workspace_path`, usually `/workspace`.

## Shared Services

Shared services can be configured in `vm.yaml` and are managed with the environment lifecycle. Removing an environment with `vm remove` frees active resources while preserving explicitly saved snapshots.

## State

```bash
vm save backend as before-refactor
vm revert backend before-refactor
vm package backend --output backend.tar.gz
```

## Tunnels

```bash
vm tunnel add 8080:3000 backend
vm tunnel ls backend
vm tunnel stop 8080
```
