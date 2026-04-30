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
| Open a shell | `vm shell backend` |
| Open an unnamed macOS environment | `vm shell mac` |
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

## Interaction

```bash
vm shell [name]
vm exec <name> -- <command>
vm logs [name] [--follow] [--tail <n>]
vm copy <source> <destination>
```

Examples:

```bash
vm shell backend
vm shell mac
vm exec backend -- npm test
vm logs backend --follow
vm copy ./config.json backend:/workspace/config.json
```

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
```

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
vm fleet ls [--provider <provider>] [--pattern <glob>]
vm fleet exec [--provider <provider>] -- <command>
vm secret add <name> <value>
vm secret ls
vm secret rm <name>
```
