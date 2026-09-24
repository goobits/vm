# CLI Reference

This page is the single durable inventory of public built-in `vm` commands.
Runtime `vm --help` output remains authoritative for the installed version.

```text
vm [--config <path>] [--profile <name>] <command>
```

Global options apply to every command:

| Option | Purpose |
| --- | --- |
| `--config <path>` | Load a specific `vm.yaml` |
| `--profile <name>` | Apply a named configuration profile |
| `-h`, `--help` | Show command-specific help |
| `-V`, `--version` | Show the installed version |

Use `vm help [command]...` for nested help. Managed guests may also expose
controller-approved top-level command names supplied by installed tooling. Those
names are environment-specific, so they are intentionally absent from the
built-in help and this static inventory.

## Environments

| Command | Purpose |
| --- | --- |
| `vm run <mac\|linux\|container> [as <name>] [--provider <docker\|podman\|tart>] [--image <image>] [--build <path>] [--from-snapshot <name>] [--ephemeral] [--mount <host:guest>]... [--cpu <count>] [--memory <limit>]` | Create and start an environment |
| `vm list [--all] [--raw]` | List project environments; `--all` crosses projects, `--raw` includes provider IDs |
| `vm start [environment] [--no-wait] [<fleet-options>]` | Start an existing environment |
| `vm shell [environment] [--path <path>] [-e\|--command <command>]` | Create or start an environment, then open a shell or run one shell command |
| `vm exec [--env <environment>] [<fleet-options>] -- <command>` | Start an existing environment and run one command |
| `vm logs [environment] [-f\|--follow] [-n\|--tail <lines>] [-s\|--service <service>]` | Stream environment or service logs |
| `vm copy [<fleet-options>] <source> <destination>` | Copy between host and environment paths |
| `vm stop [environment] [<fleet-options>]` | Gracefully stop an environment |
| `vm status [environment]` | Inspect runtime, storage, mounts, and resource state |
| `vm restart [environment] [<fleet-options>]` | Stop and restart an environment |
| `vm remove [environment] [--force]` | Remove an environment while preserving saved snapshots |
| `vm snapshots list [--env <environment>]` | List snapshots for an environment |
| `vm snapshots show <name> [--env <environment>]` | Inspect snapshot metadata |
| `vm snapshots create <name> [--env <environment>] [--description <text>] [--quiesce]` | Capture an environment state |
| `vm snapshots restore <name> [--env <environment>] [--yes]` | Restore a saved state after confirmation |
| `vm snapshots remove <name> [--env <environment>] [--yes]` | Delete a snapshot after confirmation |
| `vm snapshots export <name> --output <file> [--env <environment>] [--compression <level>]` | Export a snapshot as a portable archive; compression defaults to `6` |
| `vm snapshots import <archive> --name <name>` | Import a portable snapshot artifact |

`<fleet-options>` means:

```text
--all-envs [--match-provider <docker|podman|tart>] [--match <glob>]
```

Fleet options are supported by `start`, `exec`, `copy`, `stop`, and `restart`.
Provider and pattern filters require `--all-envs`. Without filters, the command
targets applicable environments with a matching project identity. An empty
selection fails without making changes.

### Target Selection

When an environment is omitted, VM prefers the configured default profile, the
canonical project environment, then the project's sole match. Interactive
commands offer a choice when several matches remain; non-interactive commands
list the candidates and stop. An environment named `docker` is still an
environment, not a provider selector.

`shell` creates a missing environment. `start`, `exec`, `status`, `logs`,
`copy`, `stop`, `restart`, `remove`, and snapshot operations require an existing
environment. Host-to-guest copy paths use `environment:/path`.

## Configuration

| Command | Purpose |
| --- | --- |
| `vm config validate` | Validate the active configuration |
| `vm config show` | Show the merged active configuration |
| `vm config render [--instance <name>]` | Render redacted provider configuration without applying it |
| `vm config get [field] [--global]` | Read one field or the complete configuration |
| `vm config set <field> <value>... [--global]` | Set a project or global field |
| `vm config unset <field> [--global]` | Remove a project or global field |
| `vm config preset [names] [--global] [--list] [--show <name>]` | Apply or inspect presets |
| `vm config profile ls` | List project profiles |
| `vm config profile set <name>` | Select the project default profile |
| `vm config ports [--fix]` | Inspect or repair configured port conflicts |
| `vm config clear [--global]` | Clear project or global configuration |

Configuration fields and examples belong in the
[Configuration Guide](configuration.md).

## Tunnels

| Command | Purpose |
| --- | --- |
| `vm tunnels add <host-port>:<guest-port> [environment]` | Start a port forward |
| `vm tunnels list [environment]` | List active forwards |
| `vm tunnels stop [port] [environment] [--all]` | Stop one or all forwards |

## Package Infrastructure

| Command | Purpose |
| --- | --- |
| `vm packages init <source-root> [--port <port>]` | Store the controller source shelf and initialize the appliance |
| `vm packages up [--engine <auto\|docker\|podman>] [--port <port>] [--registry-image <image>] [--job-image <image>]` | Reconcile the appliance and configured sources |
| `vm packages down` | Stop the appliance while preserving volumes |
| `vm packages status` | Report appliance or guest workflow health |
| `vm packages doctor [--fix]` | Diagnose or safely repair package infrastructure |
| `vm packages backups` | List appliance backups |
| `vm packages backup` | Create a private named-volume backup |
| `vm packages restore <backup-id>` | Restore a backup while services are stopped |
| `vm packages register <name-or-path>... [--ecosystem <npm\|cargo\|python>] [--repository <url>] [--branch <branch>] [--recursive]` | Register catalog metadata; successful local roots are remembered read-only |
| `vm packages list` | List registered and published package state |
| `vm packages consumers register <name> --repository <url> [--branch <branch>] --dependency <package@version>...` | Register a consumer and its internal dependencies |
| `vm packages consumers list` | List registered consumers |
| `vm packages consumers retry <name>` | Retry failed dependency updates without republishing the package |
| `vm packages consumers list --package <package>` | Show consumers and pending upgrades for one package |
| `vm packages consumers drift` | Show version drift across consumers |
| `vm packages open <source>` | Open an attested package or tool in its existing writable Docker owner; create no checkout |
| `vm packages checkout <source>` | Create or resume a guest-owned package or tool checkout |
| `vm packages release` | Release the checkout or canonical workspace containing the current directory; print durable job and phase progress |
| `vm packages cancel` | Cancel and clean the checkout containing the current directory |
| `vm packages auth (--github\|--token-file <path>\|--clear)` | Import or remove the controller Git token |

Controller commands, including `open`, run on the host. `status`, `checkout`,
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
| `vm tools refresh` | Refresh the controller tool catalog |
| `vm tools status [environment]` | Combine controller workflow/job, publication, installed, and consumable state |
| `vm tools enable <tool>...` | Select tools globally and activate them in every running managed Docker environment |
| `vm tools disable <tool>...` | Remove tools from the global selection while retaining existing managed files |
| `vm tools update [<tool>...] [--to <environment>]... [--include-stopped] [--background]` | Update VM-owned vendor tools and configured package tools across selected environments |

`enable` persists controller-global defaults, then activates each tool in every
running managed Docker environment. Future environments inherit those defaults.
Project entries with the same name override global version and update-policy
settings. `disable` stops global enrollment without deleting existing managed
files or a project-owned selection.

With no tool names, `update` refreshes Codex, Claude, and Antigravity and loads
every running managed Docker environment's effective global-plus-project
package-tool selection. Those three VM-owned names can be selected directly
without configuration. Other names filter configured package-tool selections;
they never install an unconfigured package tool. Repeat `--to` to restrict exact environments,
including Podman or Tart targets. Stopped environments remain untouched unless
`--include-stopped` is explicit. A selected tool that is not configured in any
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
| `vm system update [--version <version>] [--force]` | Update the VM installation |
| `vm system uninstall [--keep-config] [-y\|--yes]` | Remove VM from the host |
| `vm system base build <preset> --provider <docker\|podman\|tart> [--guest-os <auto\|linux\|macos>]` | Build a provider-native base |

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
| `vm plugins validate <name>` | Validate plugin configuration |
| `vm db list` | List databases and backups |
| `vm db backup [database] [name] [--all]` | Back up one or all databases |
| `vm db restore <backup> <database>` | Restore a database backup |
| `vm db export <database> <file>` | Export SQL |
| `vm db import <file> <database>` | Import SQL |
| `vm db size` | Show database disk usage |
| `vm db reset <database> [--force]` | Drop and recreate a database |
| `vm db credentials <service>` | Show service credentials |
| `vm secrets status` | Check the secret proxy |
| `vm secrets set <name> [--stdin\|--file <path>] [--scope <scope>] [--description <text>]` | Prompt securely or read a secret from stdin or file |
| `vm secrets list` | List secret metadata without values |
| `vm secrets show <name> --reveal` | Explicitly print one secret value |
| `vm secrets remove <name> [-f\|--force]` | Delete a secret |

Plugin-backed commands depend on installed plugin support. Use
`vm help <command>` or `vm help <command> <subcommand>` for the installed
version's generated help.
