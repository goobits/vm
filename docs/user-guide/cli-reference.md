# CLI Reference

This page is the single durable inventory of public `vm` commands. Runtime
`vm --help` output remains authoritative for the installed version.

```text
vm [--config <path>] [--profile <name>] [--dry-run] <command>
```

Global options apply to every command:

| Option | Purpose |
| --- | --- |
| `--config <path>` | Load a specific `vm.yaml` |
| `--profile <name>` | Apply a named configuration profile |
| `--dry-run` | Describe the operation without changing state |
| `-h`, `--help` | Show command-specific help |
| `-V`, `--version` | Show the installed version |

## Environments

| Command | Purpose |
| --- | --- |
| `vm run <mac\|linux\|container> [as <name>] [--provider <docker\|podman\|tart>] [--image <image>] [--build <path>] [--from-snapshot <name>] [--ephemeral] [--mount <host:guest>]... [--cpu <count>] [--memory <limit>]` | Create and start an environment |
| `vm list [--all] [--raw]` | List project environments; `--all` crosses projects, `--raw` includes provider IDs; alias: `vm ls` |
| `vm start [environment] [--no-wait] [<fleet-options>]` | Start an existing environment |
| `vm shell [environment] [--path <path>] [-e\|--command <command>]` | Create or start an environment, then open a shell or run one shell command; alias: `vm ssh` |
| `vm exec [environment] [<fleet-options>] -- <command>` | Start an existing environment and run one command |
| `vm logs [environment] [-f\|--follow] [-n\|--tail <lines>] [-s\|--service <service>]` | Stream environment or service logs |
| `vm copy [<fleet-options>] <source> <destination>` | Copy between host and environment paths |
| `vm stop [environment] [<fleet-options>]` | Gracefully stop an environment; aliases: `down`, `halt` |
| `vm status [environment]` | Inspect runtime, storage, mounts, and resource state |
| `vm restart [environment] [<fleet-options>]` | Stop and restart an environment |
| `vm remove [environment] [--force]` | Remove an environment while preserving saved snapshots; aliases: `rm`, `destroy` |
| `vm save [environment] as <snapshot> [--description <text>] [--quiesce] [--force]` | Save an environment state |
| `vm revert [environment] <snapshot> [--force]` | Restore a saved environment state |
| `vm package [environment] [-o\|--output <file>] [--compress <1-9>] [--build <path>]` | Export an environment or build context as a portable artifact |

`<fleet-options>` means:

```text
--fleet [--provider <docker|podman|tart>] [--pattern <glob>]
```

Fleet options are supported by `start`, `exec`, `copy`, `stop`, and `restart`.
Provider and pattern filters require `--fleet`. Without filters, the command
targets all applicable managed environments.

When an environment is omitted, VM prefers the configured default profile, the
canonical project environment, then the project's sole match. Interactive
commands offer a choice when several matches remain; non-interactive commands
list the candidates and stop. An environment named `docker` is still an
environment, not a provider selector.

`shell` creates a missing environment. `start`, `exec`, `status`, `logs`,
`copy`, `stop`, `restart`, `remove`, `save`, and `revert` require an existing
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
| `vm tunnel add <host-port>:<guest-port> [environment]` | Start a port forward |
| `vm tunnel ls [environment]` | List active forwards |
| `vm tunnel stop [port] [environment] [--all]` | Stop one or all forwards |

## Package Infrastructure

Primary tool-release workflow:

```bash
# Once
vm tools enable typemill codeatlas

# Daily producer workflow
vm packages release
```

### Advanced Commands

| Command | Purpose |
| --- | --- |
| `vm packages init <source-root> [--port <port>]` | Store the controller source shelf and initialize the appliance |
| `vm packages up [--engine <auto\|docker\|podman>] [--port <port>] [--registry-image <image>] [--job-image <image>]` | Reconcile the appliance and configured sources |
| `vm packages down` | Stop the appliance while preserving volumes |
| `vm packages status` | Report appliance or guest workflow health |
| `vm packages doctor [--fix]` | Diagnose or safely repair package infrastructure |
| `vm packages backup` | Create a private named-volume backup |
| `vm packages backups` | List appliance backups |
| `vm packages restore <backup-id>` | Restore a backup while services are stopped |
| `vm packages register <name-or-path>... [--ecosystem <npm\|cargo\|python>] [--repository <url>] [--branch <branch>] [--recursive]` | Register catalog metadata; successful local roots are remembered read-only |
| `vm packages list` | List registered and published package state |
| `vm packages consumer register <name> --repository <url> [--branch <branch>] --dependency <package@version>...` | Register a consumer and its internal dependencies |
| `vm packages consumer list` | List registered consumers |
| `vm packages consumers <package>` | Show consumers and pending upgrades for one package |
| `vm packages drift` | Show version drift across consumers |
| `vm packages open <package-or-tool>` | Open the attested original source in its existing writable Docker owner; create no checkout |
| `vm packages checkout <package-or-tool>` | Create or resume a guest-owned source checkout |
| `vm packages release` | Release the checkout or canonical workspace containing the current directory |
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

Primary enrollment is `vm tools enable <tool>...`; it selects tools globally
and immediately activates them in running environments.

### Advanced Commands

| Command | Purpose |
| --- | --- |
| `vm tools register <name> --repository <url> [--branch <branch>] [--kind <binary\|collection>]` | Register a trusted tool source |
| `vm tools list` | List registered tools and publication state |
| `vm tools show <name>` | Show one tool and its releases |
| `vm tools refresh` | Refresh the controller tool catalog |
| `vm tools status [environment]` | Combine controller, installed, and consumable state |
| `vm tools enable <tool>...` | Select tools globally and activate them in every running managed Docker environment |
| `vm tools disable <tool>...` | Remove tools from the global selection while retaining existing managed files |
| `vm tools update [<tool>...] [--to <environment>]... [--include-stopped] [--background]` | Update configured tools across selected environments |

`enable` persists controller-global defaults, then activates each tool in every
running managed Docker environment. Future environments inherit those defaults.
Project entries with the same name override global version and update-policy
settings. `disable` stops global enrollment without deleting existing managed
files or a project-owned selection.

With no tool names, `update` loads every running managed Docker environment's
effective global-plus-project tool selection. Tool names filter those configured
selections; they never install an unconfigured tool. Repeat `--to` to restrict exact environments,
including Podman or Tart targets. Stopped environments remain untouched unless
`--include-stopped` is explicit. A selected tool that is not configured in any
successfully loaded target is rejected.

Explicit updates include prompt-policy releases while respecting persisted
`off` policies for ordinary upgrades. Reconciliation repairs package routing,
the base-owned Codex runtime, and managed links without recreating the primary
environment. Active agent sessions do not hot-reload updated skills.

## Diagnostics And System Management

| Command | Purpose |
| --- | --- |
| `vm doctor [--fix] [--clean] [--prune-pnpm-store] [--container <environment>]` | Diagnose or repair engine, configuration, and pnpm-store issues |
| `vm system update [--version <version>] [--force]` | Update the VM installation |
| `vm system uninstall [--keep-config] [-y\|--yes]` | Remove VM from the host |
| `vm system base build <preset> --provider <docker\|tart> [--guest-os <auto\|linux\|macos>]` | Build a provider-native base |
| `vm system base validate <preset> [--provider <docker\|tart\|all>] [--rebuild-docker-base] [--build-tart-base]` | Validate provider base workflows |

`vm config validate` is read-only. `vm config render` redacts secrets and host
paths. Ordinary cleanup and repair preserve managed data unless a command
explicitly states otherwise.

## Plugins, Databases, And Secrets

| Command | Purpose |
| --- | --- |
| `vm plugin ls` | List installed plugins |
| `vm plugin info <name>` | Show plugin details |
| `vm plugin install <path>` | Install a plugin |
| `vm plugin rm <name>` | Remove a plugin |
| `vm plugin new <name> --type <preset\|service>` | Scaffold a plugin |
| `vm plugin validate <name>` | Validate plugin configuration |
| `vm db ls` | List databases and backups |
| `vm db backup [database] [name] [--all]` | Back up one or all databases |
| `vm db restore <backup> <database>` | Restore a database backup |
| `vm db export <database> <file>` | Export SQL |
| `vm db import <file> <database>` | Import SQL |
| `vm db size` | Show database disk usage |
| `vm db reset <database> [--force]` | Drop and recreate a database |
| `vm db credentials <service>` | Show service credentials |
| `vm secret status` | Check the secret proxy |
| `vm secret add <name> <value> [--scope <scope>] [--description <text>]` | Store a secret |
| `vm secret ls [--show-values]` | List secrets |
| `vm secret rm <name> [-f\|--force]` | Delete a secret |
| `vm secret interactive` | Add a secret without placing its value in shell history |

Plugin-backed commands depend on installed plugin support. Use
`vm help <command>` or `vm help <command> <subcommand>` for the installed
version's generated help.
