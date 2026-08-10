# CLI Reference

```text
vm - Humane Virtual Environments

USAGE:
  vm <command> [subject] [options]
```

## Everyday Lifecycle

| Goal | Command |
| --- | --- |
| Run a Linux environment | `vm run linux as backend` |
| Run a macOS environment | `vm run mac as xcode` |
| Run a container | `vm run container as redis` |
| List this project's environments | `vm list` |
| List every environment | `vm list --all` |
| Open the default environment | `vm ssh` or `vm shell` |
| Start the default environment | `vm start` |
| Open an unnamed macOS environment | `vm shell mac` |
| Inspect one environment | `vm status container` |
| Stop an environment | `vm stop backend` |
| Restart an environment | `vm restart backend` |
| Remove an environment | `vm remove backend` |

### `vm run`

```bash
vm run <mac|linux|container> [as <name>] [options]
```

Options:

| Option | Purpose |
| --- | --- |
| `--provider <docker|podman|tart>` | Override engine routing |
| `--image <image>` | Use a specific image or distro |
| `--build <path>` | Build from a local Dockerfile/context |
| `--from-snapshot <name>` | Clone from a saved state |
| `--cpu <count>` | Limit CPU count |
| `--memory <limit>` | Limit memory |
| `--mount <host:guest>` | Mount a host path |
| `--ephemeral` | Create a throwaway environment |

Examples:

```bash
vm run linux as backend
vm run mac as xcode
vm run mac
vm run container as redis-cache --image redis:7
vm run linux as secure-node --provider tart
```

## Target Selection

Lifecycle and interaction commands accept an environment name but do not
require one. With no name, `vm` chooses in this order:

1. The project's configured default profile, when profiles are present.
2. The canonical project environment for that configuration.
3. The only matching project environment when the canonical one is absent.
4. An interactive choice when multiple matches remain.

Non-interactive commands fail with the candidate names instead of guessing.
Installed providers do not cause a prompt by themselves.

`vm start docker` targets an environment or configured profile named `docker`;
it does not select Docker merely because that provider is installed. Use
`--profile docker` explicitly for a profile or `vm run ... --provider docker`
for an advanced provider override.

## Interaction

```bash
vm shell [name]
vm ssh [name]
vm exec [name] -- <command>
vm logs [name] [--follow] [--tail <n>]
vm copy <source> <destination>
```

`vm shell` and its `vm ssh` alias create a missing environment directly from
the selected `vm.yaml` configuration, then connect. Existing stopped
environments are started first. Other interaction commands require an existing
environment.

Examples:

```bash
vm shell backend
vm shell mac
vm exec backend -- npm test
vm logs backend --follow
vm copy ./config.json backend:/workspace/config.json
```

`vm shell` and `vm ssh` create a missing selected environment, while all three
commands start an existing stopped environment and wait until it is ready.
`vm exec` never creates one. `vm logs` and `vm copy` do not change lifecycle
state.

Targeted container status reports the generated Compose path, writable-layer
size, named-volume usage, `/tmp` usage, memory and PID peaks, mounts, logging,
and lifecycle settings. Named-volume usage is separate from writable-layer
size.

## State

```bash
vm save [name] as <snapshot>
vm revert [name] <snapshot>
vm package [name] [--output <file>] [--compress <1-9>]
```

Examples:

```bash
vm save backend as stable
vm revert backend stable
vm package backend --output backend.tar.gz
```

`vm remove` removes active environment resources and preserves explicitly saved snapshots.

## Config

```bash
vm config validate
vm config show
vm config render [--instance <name>]
vm config get [field]
vm config set <field> <value...>
vm config unset <field>
vm config preset [names]
vm config profile ls
vm config profile set <name>
vm config ports --fix
vm config clear
```

## Tunnels

```bash
vm tunnel add <host>:<guest> [name]
vm tunnel ls [name]
vm tunnel stop [port] [name] [--all]
```

Examples:

```bash
vm tunnel add 8080:3000 backend
vm tunnel ls backend
vm tunnel stop 8080
```

## System

```bash
vm system update [--version <version>] [--force]
vm system uninstall [--keep-config] [--yes]
vm system registry status
vm system registry add [--type <python|npm|cargo>]
vm system registry ls
vm system registry rm [--force]
vm system base build <preset> --provider <docker|tart> [--guest-os <auto|linux|macos>]
vm system base validate <preset> [--provider <docker|tart|all>]
```

## Doctor

```bash
vm doctor [--fix] [--clean]
vm doctor --prune-pnpm-store [--container <environment>]
```

`vm config validate` is read-only. `vm config render` also performs validation,
redacts environment values and host paths, and does not contact the provider.
pnpm pruning is explicit and never runs during create, start, or bootstrap.

## Package Infrastructure

```bash
vm packages up [--runtime <auto|docker|tart>]
vm packages status
vm packages doctor
vm packages backup
vm packages backups
vm packages restore <backup-id>
vm packages checkout <package> --agent <agent> --task <task>
vm packages submit <checkout-id>
vm packages cancel <checkout-id>
vm packages cleanup <checkout-id>
vm packages integrate <submission-id>
vm packages publish <submission-id> --push-source
vm packages drift
vm packages rollout <package>@<version> --to <consumer>
```

See [Package Infrastructure](package-infrastructure.md) for the provider
boundary, registration, credentials, release workflow, and recovery model.

## Plugins

```bash
vm plugin ls
vm plugin info <name>
vm plugin install <path>
vm plugin rm <name>
vm plugin new <name> --type <preset|service>
vm plugin validate <name>
```

Plugin-backed commands are flat at the top level:

```bash
vm db ls
vm db backup <database>
vm fleet exec [--provider <provider>] -- <command>
vm secret add <name> <value>
vm secret ls
vm secret rm <name>
vm secret interactive
```

Prefer `vm secret interactive` when a value should not appear in shell history.
