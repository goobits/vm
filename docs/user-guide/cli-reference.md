# CLI Reference

This guide explains command behavior and targeting. The
[generated command inventory](command-inventory.md) lists every public built-in
command directly from the parser. `vm --help` shows options for the installed
version.

```text
vm [--project <path-or-id> | --config <path>] [--profile <name>] <command>
```

Global options apply to every command:

| Option | Purpose |
| --- | --- |
| `--config <path>` | Load a specific `vm.yaml` |
| `--project <path-or-id>` | Select a registered project ID or directory containing `vm.yaml` |
| `--profile <name>` | Apply a named configuration profile |
| `--quiet` | Suppress progress while retaining requested data and errors |
| `--no-color` | Disable colored terminal output |
| `-h`, `--help` | Show command-specific help |
| `-V`, `--version` | Show the installed version |

Use `vm help [command]...` for nested help. Installed tools run as normal guest
executables, either inside the environment or through `vm exec -- TOOL ...`.

## Environments

Snapshot names are unique in the selected project. Read, export, and remove
commands use project configuration, so they work after an environment is removed.
`vm snapshots list --json` and `vm snapshots show NAME --json` emit structured
read results. Snapshot create, restore, remove, export, and import also accept
`--json` for structured outcomes.
`--env` filters by the environment recorded at capture. Restore requires the
recorded source environment. Docker and Podman also require a matching Compose volume plan.

Snapshot metadata lists included services and named volumes and identifies each
excluded bind, anonymous, external, custom named, or temporary mount from normalized Compose
configuration. Snapshot export checksums every included file and publishes the archive atomically.
An existing destination is preserved unless `--overwrite` is set. Import verifies
the checksums and recorded provider before installing the snapshot.
Docker and Podman capture the normalized Compose project. Tart captures a stopped
local VM through Tart's native `.tvm` export. Tart host shares, including the
workspace, are excluded; stop the Tart environment before capture or restore.
Tart restore imports a staged VM, preserves the original until replacement is
installed, and leaves it named in the error if automatic recovery fails.
Native snapshots retain the captured runtime's configuration fingerprint. Restoring
one does not certify a newer project configuration: start reports drift when the
restored runtime and selected configuration differ. Native archives require this
fingerprint for import and restore.
Container capture removes its temporary image tag after saving the archive,
including on capture failure; cleanup errors identify the exact tag to remove.

`<fleet-options>` means:

```text
--all-envs [--match-provider <docker|podman|tart>] [--match <glob>]
```

Fleet options are supported by `start`, `exec`, `stop`, `status`, `restart`, and `remove`.
Provider and pattern filters require `--all-envs`. Without filters, the command
targets applicable environments owned by the selected project configuration.
Environments with the same project name in another configuration are excluded. An empty
selection fails without making changes.

`start --all-envs` creates missing declared environments and leaves running ones
alone. `remove` freezes and displays its exact target set before confirmation;
`--yes` permits a reviewed noninteractive removal. Docker and Podman retain
volumes and snapshots unless `--delete-data` is explicit. With that option,
only unreferenced instance volumes carrying the exact project/config ownership
labels are deleted. Shared volumes remain. Tart removal requires `--delete-data`
because deleting a Tart VM also deletes its disk.

### Target Selection

When an environment is omitted, VM uses the project's default declared
environment, its sole declaration, or its sole existing runtime. If several
matches remain, it lists the candidates and stops. A profile selects a config
overlay; it does not name an environment. An environment named `docker` is an
environment, not a provider selector. Explicit environment names resolve only
inside the selected project.

`create` records a named environment in the project configuration and leaves its
runtime stopped. `start` provisions a missing declared environment. `shell` and `exec` require
a running environment; `status`,
`logs`, `copy`, `stop`, `restart`, `remove`, and snapshot operations require an
existing environment. Host-to-guest copy paths use `environment:/path`.

`vm logs [ENV] --json-lines` emits one JSON object per log record and a final
`result` event, including failures. Each record identifies its environment,
service, output stream, and provider timestamp when available. Application
bytes, including invalid UTF-8 or JSON-like text, are carried as base64 in
`data_base64`; decode that field to recover the original bytes. `--follow` and
`--tail` work with this format.

## Configuration

Configuration fields and examples belong in the
[Configuration Guide](configuration.md).

## Tunnels

The relay supports Docker or Podman environments. `localhost` binds to `127.0.0.1`; use an explicit IPv4 address to expose a listener. Remote endpoints accept IPv4 or DNS names reachable from the environment's container network. IPv6 endpoints are rejected. `tunnels list` and `tunnels close` read the selected project's recorded relays and still work after the source environment is removed. If a tunnel name matches multiple relays, select one with `--env` and `--provider docker|podman`.

`vm tunnels list --json` returns typed project-owned relay views. It omits
configuration paths and relay container identifiers while retaining the
endpoints and an opaque tunnel ID.

## Package Infrastructure

Controller commands, including `open`, run on the host. `service status`, `checkout`,
`release`, and `cancel` also have scoped behavior inside managed guests.
Language packages are published privately and upgraded through registered
consumer rollout; they are not installed indiscriminately into every
environment.

Local-path registration stores the physical Git root in controller-global
`packages.canonical_sources`; URL-only registration does not grant workspace
release authority. Managed recursive shelves remain under
`packages.source_roots`.

The [Package Infrastructure Guide](package-infrastructure.md) owns setup,
release, security, recovery, and consumer workflow details.

## Managed Tools

`enable` persists controller-global defaults, then activates each tool in every
running managed environment. Future environments inherit those defaults.
Project entries with the same name override global version and update-policy
settings. `disable` stops global enrollment without deleting existing managed
files or a project-owned selection.

With no tool names, `update` refreshes Codex, Claude, and Antigravity and loads
the selected environment's effective global-plus-project package-tool selection.
Use `--all-envs` for all running environments in the current project. Those three VM-owned names can be selected directly
without configuration. Other names filter configured package-tool selections;
they never install an unconfigured package tool. Repeat `--env` to select exact environments,
including Podman or Tart targets. Stopped environments remain untouched unless
`--include-stopped` is explicit; updates are deferred without starting them. A selected tool that is not configured in any
successfully loaded target is rejected.

Vendor updates download the declared official HTTPS installer, stage and
version-check its result, atomically activate managed links, and roll back on
failure. Codex also requires `codex-code-mode-host`. Vendor tools do not use
`enable`, project configuration, or package publication.

Explicit package updates include prompt-policy releases while respecting persisted
`off` policies for ordinary upgrades. Reconciliation repairs package routing,
base-owned vendor runtimes, and managed links without recreating the primary
environment. Active agent sessions do not hot-reload updated skills.

## Diagnostics And System Management

`vm config validate` is read-only. `vm config render` redacts secrets and host
paths. Ordinary cleanup and repair preserve managed data unless a command
explicitly states otherwise.

Saved snapshots appear in `system storage list` with their project owner;
`vm system storage list --json` emits a structured read result. Use
`vm system storage remove ID --json --yes` for a structured removal outcome.
Use `vm snapshots remove` from that project to reclaim a snapshot; the generic storage
removal command does not delete saved snapshots. Recorded Tart VM disks are
removable only when the owning project configuration is gone and Tart reports
a stopped local VM.

## Plugins, Databases, And Secrets

Database commands require an enabled `services.postgresql` definition. A named environment can override that service under `environments.<name>.services.postgresql` to select a different database. Backups are scoped to the project and environment. Commands that name a database require the exact configured name.

Secret reads and listings show metadata by default. `vm secrets show NAME
--reveal` is the explicit path for writing a value to stdout. Installed plugins
extend presets and service definitions; they do not add built-in command names.
`vm plugins list --json` returns installed plugin metadata. `vm plugins show
NAME --json` adds a content summary, omitting file paths, environment values,
commands, and volume mappings. Both use the standard versioned JSON result
envelope, including failures.
