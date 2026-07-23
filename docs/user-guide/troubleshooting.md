# Troubleshooting

Start with VM-owned diagnostics. Avoid direct Docker cleanup until you know which
container and volumes belong to the project.

```bash
vm doctor
vm config validate
vm config render
vm status
vm status docker
vm logs -n 100
```

`vm config render` is read-only and redacts environment values and host paths.
`vm status` lists environments; a targeted status such as `vm status docker`
adds runtime, storage, mount, and logging evidence.

## Safety Rules

Do not use these as general VM fixes:

```text
docker system prune
docker volume prune
docker compose down -v
rm -rf node_modules
```

They operate outside the VM tool's ownership policy and can remove unrelated or
kept data. `vm doctor --clean` only considers dangling volumes that VM labeled as
both managed and disposable.

Before any recreation, classify files that exist only in the container writable
layer. Common examples are SSH keys, SOPS/age keys, CLI credentials, untracked
documents, and process-manager state. Move credentials to a secure canonical
source; do not copy private keys into `/workspace`.

## Create And Connect

`vm create` owns creation and provisioning. `vm ssh` only connects; it does not
silently create, start, rebuild, refresh packages, or launch commands.

```bash
vm create
vm ssh
```

If the environment is stopped:

```bash
vm start
vm ssh
```

For a provider-specific project:

```bash
vm create docker
vm start docker
vm ssh docker
```

Use `LOG_LEVEL=DEBUG vm create` when creation fails. Check the first failing task,
not only the final provider error.

### Missing package.json

Bootstrap detects the package manager only when the corresponding project files
exist. It does not run `npm install` merely because no pnpm lockfile was found.
If dependency installation reports a missing `/workspace/package.json`, verify
that the current VM binary includes the fingerprinted bootstrap flow and render
the loaded project configuration:

```bash
vm --version
vm config show
vm config render
```

## Runtime And Storage

Use a targeted status after a representative workload:

```bash
vm status docker
```

The report separates:

- container writable-layer and root-filesystem size
- named-volume usage
- `/tmp` storage type, capacity, and use
- current and peak PIDs
- peak memory and configured memory limit
- generated Compose path and mount topology
- log driver/rotation, restart policy, and stop timeout

Volume sizes are separate because Docker's writable-layer size excludes named
volumes. For canary limits, retain at least 30% memory and PID headroom. Keep
representative `/tmp` use below 70% of its capacity.

### Slow macOS Workspace

Keep source under the `/workspace` host bind, but move large container-only,
high-churn data to configured named volumes. Typical Node/Playwright policy:

```yaml
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
```

Do not permanently mount all of `/home/developer/.cache`. Add a narrow volume
only after measurement identifies a large, high-churn owner.

The container volume hides host `/workspace/node_modules` only inside the
container. The host directory is not deleted. Confirm host editor behavior and
get owner approval before cleaning that generated directory.

### pnpm Store Growth

Measure first. When unreferenced packages justify maintenance, stop installs in
all instances that share the store, then run:

```bash
vm doctor --prune-pnpm-store
```

Pruning is never part of create, start, or bootstrap because later branch work
may need to download removed packages again.

## Safe Recreation

Recreation is required to reclaim an existing writable layer; restart alone does
not reclaim it. Code changes and render-only tests do not require this preflight,
but applying them to a live environment does.

1. Run `vm config validate` and `vm config render`.
2. Run `vm status docker` and record storage, memory, PIDs, `/tmp`, and mounts.
3. Confirm source, worktrees, history, AI configuration, credentials, documents,
   and any important database are mounted or recoverable.
4. Confirm no agents, claims, tests, browsers, installs, watchers, PM2 jobs, or
   uncheckpointed work remain active.
5. Stop through `vm stop docker`.
6. Recreate through `vm create docker --force`; do not delete named volumes.
7. Start only explicitly requested development services.
8. Repeat the workload and targeted status checks.

Do not restore Zellij sockets, PM2 PIDs, browser processes, or pane commands.
Checkpoint work through source control and coordination state.

### Existing UUID Volumes

Older VM versions generated UUID-scoped Compose project names. Their shell
history and database volumes do not automatically attach to the new stable
names. Inventory them before recreation and transfer only valuable data through
the owning backup/restore workflow. Do not seed new dependency or browser
volumes from a large old writable layer.

After migration, stable physical volume names survive repeated container and
Compose recreation. `nocopy: true` prevents Docker from populating new volumes
from image or bind contents.

### Rollback

Restore the previous `vm.yaml` and recreate the container against the preserved
source binds. Rollback must not depend on restoring source code from backup.
Keep pre-migration database/history volumes until validation and owner approval
are complete.

## Multiple Instances And Fleet

Fleet remains available for explicit actions across selected environments:

```bash
vm status
vm fleet exec --provider docker --pattern 'api-*' -- uname -a
vm fleet stop --provider docker --pattern 'api-*'
```

Use instance-scoped `node_modules`; never share it across branches running at the
same time. A platform-scoped pnpm store and Playwright cache may be shared within
one project/architecture, but do not prune either while another instance uses it.
Each instance must have distinct published host ports.

## Logs

```bash
vm logs
vm logs -n 200
vm logs -f
vm logs --service postgresql
```

Configure bounded container logs in `vm.yaml`:

```yaml
vm:
  logging:
    driver: local
    max_size: 20m
    max_files: 5
```

Validate the effective driver and options with targeted `vm status`.

## Port Conflicts

```bash
lsof -i :3000
vm config ports --fix
vm config render
```

Named instances share the project's default port declarations unless configured
otherwise. Multiple complete stacks therefore need distinct host ports.

## Provider Problems

### Docker

```bash
docker version
docker ps
vm doctor
```

On macOS or Windows, start Docker Desktop and wait for the daemon to become
ready. On Linux, fix socket access through the platform's Docker group policy;
`vm doctor --fix` can handle supported local cases.

### Tart

```bash
tart --version
tart list
vm status tart
```

Tart does not support Docker container storage, PID, logging, or tmpfs settings.
Use provider profiles when Docker and Tart need different configuration.

## Reporting A Problem

Include:

1. `vm --version` and provider version.
2. The failing command and first error.
3. `vm doctor` output.
4. Redacted `vm config render` output.
5. Targeted `vm status <provider|container>` output.
6. Relevant `vm logs -n 100` output.

Never attach raw environment values, private keys, credential files, or an
unredacted generated Compose file.
