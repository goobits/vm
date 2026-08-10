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
vm config render
vm config get vm.memory
vm config set vm.memory 8192
vm config unset vm.swappiness
vm config ports --fix
vm config clear
```

`vm config validate` never edits configuration. `vm config render` renders the
selected config and profile without contacting the provider; environment values
and host paths are redacted.

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

## Linux Tart With Docker

Use `vibe-tart` when you want the isolation of a full Linux VM with Docker
Engine inside it:

```bash
vm config preset vibe-tart
vm ssh
```

The preset selects its Linux Tart profile by default. The equivalent minimal
provider configuration is:

```yaml
provider: tart
tart:
  guest_os: linux
  disk_size: 80
  ssh_user: admin
  install_docker: true
vm:
  box: vibe-tart-linux-base
  cpus: 8
  memory: 16384
```

`vm ssh` creates the environment when missing. If the versioned Linux base is
not local, `vm` pulls it into the Tart cache or builds it when the published
image is unavailable. Docker runs directly against the Linux guest kernel, so
Colima is not part of this path.

No custom network is required. Vibe presets do not add `spacebase`; configure
`networking` only when the project explicitly needs a named network.

## Project Mounts

The directory containing `vm.yaml` is mounted read-write at
`project.workspace_path` by default. Change only its access with
`workspace_access`, and add any number of explicit mounts at the top level:

```yaml
project:
  workspace_path: /workspace
  workspace_access: read_only

mounts:
  - source: ../shared-auth
    target: /packages/auth
    access: read_only
  - source: ../shared-ui
    target: /packages/ui
    access: read_write
```

Relative sources resolve from `vm.yaml`. Docker and Tart enforce the same access
values. Read-only Node workspaces keep `node_modules` writable in guest-managed
storage; language caches and Rust build output remain outside the source mount.

## Container Storage And Bootstrap

Container projects can move high-churn data off host binds and the writable
layer while keeping source mounted at `/workspace`. This policy is opt-in and
belongs in `vm.yaml`, not in agent prompts or ad hoc bootstrap scripts. Named
volumes, tmpfs, container limits, and container logging do not apply to Tart.

```yaml
vm:
  memory: 20gb
  pids_limit: 4096
  stop_grace_period: 60
  logging:
    driver: local
    max_size: 20m
    max_files: 5

storage:
  volumes:
    node_modules:
      target: /workspace/node_modules
      scope: instance
      nocopy: true
    pnpm_store:
      target: /home/developer/.local/share/pnpm/store
      scope: platform
      nocopy: true
    playwright_browsers:
      target: /home/developer/.cache/ms-playwright
      scope: platform
      nocopy: true
  tmpfs:
    - target: /tmp
      size: 4g
      mode: "1777"

bootstrap:
  dependencies: true
  playwright:
    browsers: [chromium, firefox, webkit]
```

Scopes control stable volume sharing:

- `instance` isolates mutable branch or instance state such as `node_modules`.
- `project` shares data across one project's instances.
- `platform` shares data within the project and container OS/architecture.

Instance names do not rewrite configured host ports. Running complete instances
concurrently requires a profile or config with distinct application and service
host ports for each instance. Worktrees can coexist without separate ports when
only one stack runs at a time.

Bootstrap installs locked dependencies only when `node_modules` is empty or its
lockfile/toolchain fingerprint changes. Configured Playwright families are
installed for every resolved Playwright version only when their fingerprint
changes. Bootstrap does not start tests, browsers, watchers, agents, or terminal
sessions.

The root `node_modules` volume contains pnpm's primary virtual store.
Package-level symlink directories in a workspace may remain on the source bind.

`vm` also gives Docker environments a persistent platform-scoped home cache
and applies the same cache layout inside Tart. Cargo targets and Node, Go,
Python, uv, Corepack, npm, and Playwright caches stay out of the source bind.
Docker keeps them across container recreation; Tart keeps them on the guest
disk. Explicit values in `environment` override these generated defaults, so a
Tart-specific `CARGO_TARGET_DIR` is not needed.

## macOS Tart Guests With Docker

Tart does not support nested virtualization for macOS guests. For Docker inside
a macOS Tart guest, `vm` installs Docker CLI, Compose, Buildx, Colima, and QEMU,
then writes a software-emulation helper at `/workspace/start-colima`.

```yaml
tart:
  guest_os: macos
  install_docker: true
```

Start Docker in the guest with:

```bash
/workspace/start-colima
docker run --rm busybox echo run-ok
```

This uses QEMU TCG and is slower than native virtualization. For faster Docker,
use a Linux Tart guest or a controlled remote Docker daemon over SSH/TLS.

## AI Tool Provisioning

Host sync can provision the current native Antigravity (`agy`), Claude Code,
and Codex CLIs and retain their supported state:

```yaml
host_sync:
  git_config: true
  ai_tools:
    antigravity: true
    claude: true
    codex: true
```

`ai_tools: true` enables all three. Provisioning records the installed state so
unchanged tools are not refreshed on every boot. The old `gemini` key remains a
deprecated compatibility alias for `antigravity`; new configs should use
`antigravity`.

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

Shared services can be configured in `vm.yaml` and are managed with the
environment lifecycle. See the [Shared Services Guide](shared-services.md) for
the supported workflow.

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
