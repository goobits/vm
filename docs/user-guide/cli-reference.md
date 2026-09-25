# CLI Reference

This page is the single durable inventory of public built-in `vm` commands.
Runtime `vm --help` output remains authoritative for the installed version.

```text
vm [--project <path-or-id> | --config <path>] [--profile <name>] <command>
```

Global options apply to every command:

| Option | Purpose |
| --- | --- |
| `--config <path>` | Load a specific `vm.yaml` |
| `--project <path-or-id>` | Select a registered project ID or directory containing `vm.yaml` |
| `--profile <name>` | Apply a named configuration profile |
| `-h`, `--help` | Show command-specific help |
| `-V`, `--version` | Show the installed version |

Use `vm help [command]...` for nested help. Installed tools run as normal guest
executables, either inside the environment or through `vm exec -- TOOL ...`.

## Environments

| Command | Purpose |
| --- | --- |
| `vm init [path]` | Write a minimal `vm.yaml` and register the project ID without starting services |
| `vm create <name> --provider <docker\|podman\|tart> (--image <image> \| --snapshot <snapshot>) [--cpu <count>] [--memory <limit>] [--mount <host:guest>]...` | Declare and provision a stopped environment |
| `vm run <mac\|linux\|container> [as <name>] [--provider <docker\|podman\|tart>] [--image <image>] [--build <path>] [--from-snapshot <name>] [--ephemeral] [--mount <host:guest>]... [--cpu <count>] [--memory <limit>]` | Create and start an environment |
| `vm list [--all-projects] [--raw]` | List project environments; `--all-projects` crosses projects, `--raw` includes provider IDs |
| `vm start [environment...] [--no-wait] [<fleet-options>]` | Start environments, provisioning missing declarations |
| `vm shell [environment] [--path <path>] [-e\|--command <command>]` | Create or start an environment, then open a shell or run one shell command |
| `vm exec [--env <environment>]... [<fleet-options>] -- <command>` | Run one command in each selected running environment |
| `vm logs [environment] [-f\|--follow] [-n\|--tail <lines>] [-s\|--service <service>]` | Stream environment or service logs |
| `vm copy [<fleet-options>] <source> <destination>` | Copy between host and environment paths |
| `vm stop [environment...] [<fleet-options>]` | Gracefully stop environments |
| `vm status [environment...] [<fleet-options>]` | Inspect runtime, storage, mounts, and resource state |
| `vm restart [environment...] [<fleet-options>]` | Stop and restart environments |
| `vm remove [environment] [--force]` | Remove an environment while preserving saved snapshots |
| `vm snapshots list [--env <environment>]` | List project snapshots, optionally filtered by source environment |
| `vm snapshots show <name> [--env <environment>]` | Inspect snapshot metadata |
| `vm snapshots create <name> [--env <environment>] [--description <text>] [--quiesce]` | Capture an environment state |
| `vm snapshots restore <name> [--env <environment>] [--yes]` | Restore a saved state after confirmation |
| `vm snapshots remove <name> [--env <environment>] [--yes]` | Delete a snapshot after confirmation |
| `vm snapshots export <name> --output <file> [--env <environment>] [--compression <level>] [--overwrite]` | Export a snapshot as a portable archive; compression defaults to `6` |
| `vm snapshots import <archive> --name <name>` | Import a portable snapshot artifact |

Snapshot names are unique in the selected project. Read, export, and remove
commands use project configuration, so they work after an environment is removed.
`--env` filters by the environment recorded at capture. Restore requires that
source environment and a matching Compose volume plan.

Snapshot metadata lists included services and named volumes and identifies each
excluded bind, anonymous, external, custom named, or temporary mount from normalized Compose
configuration. Snapshot export checksums every included file and publishes the archive atomically.
An existing destination is preserved unless `--overwrite` is set. Import verifies
the checksums and recorded provider before installing the snapshot. Snapshot
capture and restore currently support Docker and Podman; restore requires a
captured Compose configuration and rejects unsupported snapshots before stopping
the environment.

`<fleet-options>` means:

```text
--all-envs [--match-provider <docker|podman|tart>] [--match <glob>]
```

Fleet options are supported by `start`, `exec`, `copy`, `stop`, `status`, and `restart`.
Provider and pattern filters require `--all-envs`. Without filters, the command
targets applicable environments owned by the selected project configuration.
Environments with the same project name in another configuration are excluded. An empty
selection fails without making changes.

### Target Selection

When an environment is omitted, VM uses the project's default declared
environment, its sole declaration, or its sole existing runtime. If several
matches remain, it lists the candidates and stops. A profile selects a config
overlay; it does not name an environment. An environment named `docker` is an
environment, not a provider selector. Explicit environment names resolve only
inside the selected project.

`create` records a named environment in the project configuration and leaves its
runtime stopped. `start` provisions a missing declared environment. `shell` can
create a missing environment. `exec` requires a running environment; `status`,
`logs`, `copy`, `stop`, `restart`, `remove`, and snapshot operations require an
existing environment. Host-to-guest copy paths use `environment:/path`.

## Configuration

| Command | Purpose |
| --- | --- |
| `vm config validate` | Validate the active configuration |
| `vm config show [--scope project\|user\|effective]` | Show redacted configuration with field sources |
| `vm config render [--env <name>]` | Render redacted provider configuration without applying it |
| `vm config get <field> [--scope project\|user\|effective]` | Read a redacted field and its source |
| `vm config set <field> <value>... [--scope project\|user]` | Set a typed scalar field; use `--value-json` for arrays or objects |
| `vm config unset <field> [--scope project\|user]` | Remove a field override |
| `vm config presets list\|show <name>\|apply <name>...` | Inspect or apply presets |
| `vm config profiles list\|show <name>\|set-default <name>` | Manage project profiles |
| `vm config ports [--fix]` | Inspect or repair configured port conflicts |

Configuration fields and examples belong in the
[Configuration Guide](configuration.md).

## Tunnels

| Command | Purpose |
| --- | --- |
| `vm tunnels open <name> --local localhost:<port> --remote localhost:<port> [--env <environment>]` | Open a named loopback tunnel |
| `vm tunnels list [--env <environment>]` | List active tunnels |
| `vm tunnels close <name> [--env <environment>]` | Close one named tunnel |

The relay supports loopback endpoints and Docker or Podman environments.

## Package Infrastructure

| Command | Purpose |
| --- | --- |
| `vm packages service init --source-root <source-root> [--port <port>]` | Store the controller source shelf and initialize the appliance |
| `vm packages up [--engine <auto\|docker\|podman>] [--port <port>] [--registry-image <image>] [--job-image <image>]` | Reconcile the appliance and configured sources |
| `vm packages down` | Stop the appliance while preserving volumes |
| `vm packages service status` | Report appliance or guest workflow health |
| `vm packages service doctor [--fix]` | Diagnose or safely repair package infrastructure |
| `vm packages service backups list` | List appliance backups |
| `vm packages service backups create` | Create a private named-volume backup |
| `vm packages service backups restore <backup-id>` | Restore a backup while services are stopped |
| `vm packages register <name-or-path>... [--ecosystem <npm\|cargo\|python>] [--repository <url>] [--branch <branch>] [--recursive]` | Register catalog metadata; successful local roots are remembered read-only |
| `vm packages list` | List registered and published package state |
| `vm packages show <name>` | Show a registered package and publication state |
| `vm packages remove <name>` | Remove a package registration while retaining published versions and source repositories |
| `vm packages consumers register <name> --repository <url> [--branch <branch>] --dependency <package@version>...` | Register a consumer and its internal dependencies |
| `vm packages consumers list` | List registered consumers |
| `vm packages consumers show <name>` | Show one registered consumer and its dependencies |
| `vm packages consumers remove <name>` | Remove a consumer registration while retaining rollout records and its repository |
| `vm packages consumers retry <name>` | Retry failed dependency updates without republishing the package |
| `vm packages consumers list --package <package>` | Show consumers and pending upgrades for one package |
| `vm packages consumers drift` | Show version drift across consumers |
| `vm packages consumers drift --package <name>` | Show version drift for one package |
| `vm packages open <source>` | Open an attested package or tool in its existing writable Docker owner; create no checkout |
| `vm packages checkout <source>` | Create or resume a guest-owned package or tool checkout |
| `vm packages release` | Release the checkout or canonical workspace containing the current directory; print durable job and phase progress |
| `vm packages cancel` | Cancel and clean the checkout containing the current directory |
| `vm packages auth login [--token-stdin\|--token-file <path>]` | Import the active GitHub token or read a secure input |
| `vm packages auth status` | Report whether a Git token is configured |
| `vm packages auth logout` | Remove the controller Git token |

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

| Command | Purpose |
| --- | --- |
| `vm tools register <name> --repository <url> [--branch <branch>] [--kind <binary\|collection>]` | Register a trusted tool source |
| `vm tools list` | List VM-owned vendor tools and registered package tools |
| `vm tools show <name>` | Show one vendor definition or package tool and its releases |
| `vm tools remove <name>` | Remove a disabled managed tool registration while retaining published artifacts |
| `vm tools refresh` | Refresh the controller tool catalog |
| `vm tools status [--env NAME]` | Combine controller workflow/job, publication, installed, and consumable state |
| `vm tools enable <tool>...` | Select tools globally and activate them in running managed environments |
| `vm tools disable <tool>...` | Remove tools from the global selection while retaining existing managed files |
| `vm tools update [<tool>...] [--env <environment>...] [--all-envs] [--include-stopped] [--background]` | Update selected environments in the current project |

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

| Command | Purpose |
| --- | --- |
| `vm doctor [--fix] [--clean] [--prune-pnpm-store] [--container <environment>]` | Diagnose or repair engine, configuration, and pnpm-store issues |
| `vm system info` | Show installation version and executable path |
| `vm system update [--version <version>]` | Update the VM installation |
| `vm system uninstall [--delete-config] [-y\|--yes]` | Remove installer-managed entries; configuration deletion is explicit |
| `vm system images build <preset> --provider <docker\|podman\|tart> [--guest-os <auto\|linux\|macos>]` | Build a provider-native base |
| `vm system storage list` | Show verified VM-owned volumes and images on the configured container provider, with owner and deletion status |
| `vm system storage remove <resource-id> [--yes]` | Remove one exact, unreferenced disposable resource after confirmation |

`vm config validate` is read-only. `vm config render` redacts secrets and host
paths. Ordinary cleanup and repair preserve managed data unless a command
explicitly states otherwise.

## Plugins, Databases, And Secrets

| Command | Purpose |
| --- | --- |
| `vm plugins list` | List installed plugins |
| `vm plugins show <name>` | Show plugin details |
| `vm plugins install <path>` | Install a plugin |
| `vm plugins remove <name>` | Remove a plugin |
| `vm plugins create <name> --kind <preset\|service>` | Scaffold a plugin |
| `vm plugins validate <path-or-name>` | Validate a local plugin source or installed plugin |
| `vm db list` | List databases and backup counts |
| `vm db status <database>` | Show database size and backup count |
| `vm db backups list [--database <database>]` | List retained backups |
| `vm db backups create <name> (--database <database>\|--all)` | Back up one or all databases |
| `vm db backups restore <backup> --database <database> [--yes]` | Restore a database backup |
| `vm db backups remove <backup> [--yes]` | Remove one retained backup |
| `vm db export <database> --output <file> [--overwrite]` | Export SQL |
| `vm db import <database> --file <file> [--yes]` | Import SQL |
| `vm db reset <database> [--yes]` | Drop and recreate a database after confirmation |
| `vm db credentials <service> [--reveal]` | Show redacted credentials status or reveal the value |

| `vm secrets status` | Check the secret proxy |
| `vm secrets set <name> [--stdin\|--file <path>] [--scope <scope>] [--description <text>]` | Prompt securely or read a secret from stdin or file |
| `vm secrets list` | List secret metadata without values |
| `vm secrets show <name> --reveal` | Explicitly print one secret value |
| `vm secrets remove <name> [-f\|--force]` | Delete a secret |

Database commands currently operate on the configured global PostgreSQL service.

Plugin-backed commands depend on installed plugin support. Use
`vm help <command>` or `vm help <command> <subcommand>` for the installed
version's generated help.
