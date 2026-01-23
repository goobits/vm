# CLI Reference

All `vm` commands with usage examples and expected output. Use this as a reference when building workflows or troubleshooting issues.

## Quick Reference

| Task | Command |
|------|---------|
| Create/start VM | `vm start` |
| Stop VM | `vm stop` |
| Connect to VM | `vm ssh` |
| Check status | `vm status [<vm>]` |
| View logs | `vm logs [-f]` |
| Wait for services | `vm start --wait` |
| Destroy VM | `vm destroy` |
| Run command | `vm exec <command>` |
| Fleet list | `vm fleet list` |
| **Snapshots** | |
| Create snapshot | `vm snapshot create <name>` |
| Restore snapshot | `vm snapshot restore <name>` |
| Export snapshot | `vm snapshot export <name>` |
| Import snapshot | `vm snapshot import <file>` |
| **Configuration** | |
| Validate config | `vm config validate` |
| Apply preset | `vm config preset <name>` |
| **Port Management** | |
| Forward port | `vm tunnel create <host>:<container>` |
| List tunnels | `vm tunnel list` |
| Stop tunnel | `vm tunnel stop [port]` |
| **Database** | |
| Backup database | `vm db backup <name>` |
| Restore database | `vm db restore <backup> <db>` |
| **System** | |
| Health check | `vm doctor` |
| Update vm tool | `vm update` |
| Generate completion | `vm completion <shell>` |

---

## Table of Contents
- [Global Options](#global-options)
- [Core Commands](#core-commands)
- [Fleet (`vm fleet`)](#fleet-vm-fleet)
- [Configuration (`vm config`)](#configuration-vm-config)
- [Port Management](#port-management)
- [Temporary VMs (`vm temp`)](#temporary-vms-vm-temp)
- [Plugins (`vm plugin`)](#plugins-vm-plugin)
- [Snapshots (`vm snapshot`)](#snapshots-vm-snapshot)
- [Database (`vm db`)](#database-vm-db)
- [Secrets Management (`vm auth`)](#secrets-management-vm-auth)
- [Package Registry (`vm pkg`)](#package-registry-vm-pkg)
- [System Management](#system-management)

---

## Global Options
These flags can be used with any command.
```bash
-c, --config <file>    # Path to a custom VM configuration file
    --dry-run          # Show what would be executed without running
-v, --verbose          # Enable verbose output
-h, --help             # Print help
-V, --version          # Print version
```

---

## Core Commands

### `vm start`
Create/configure/start an environment and open a shell.
```bash
vm start [-c <command>] [--wait]
```

**Options:**
- `-c, --command <command>`: Run a command instead of opening a shell
- `--wait`: Wait for services to be ready before continuing

### `vm stop`
Stop a running VM.
```bash
vm stop [<container>]
```

### `vm status`
List all VMs, or show details for a single VM.
```bash
vm status [<container>]
```

### `vm destroy`
Destroy a VM and all its associated resources.
```bash
vm destroy [--no-backup] [--force] [--remove-services]
```

### `vm ssh`
Connect to a VM via SSH.
```bash
vm ssh
```

### `vm exec`
Execute a command inside a VM.
```bash
vm exec <command>
```

### `vm logs`
View and follow logs from VM containers and services.

**Basic usage**:
```bash
vm logs              # Show last 50 lines of dev container
vm logs -n 100       # Show last 100 lines
```

**Follow logs in real-time**:
```bash
vm logs -f           # Follow dev container logs
vm logs --follow
```

---

## Fleet (`vm fleet`)

Bulk operations across multiple VMs and providers.

### `vm fleet list`
List instances across providers.
```bash
vm fleet list [--provider <name>] [--pattern <glob>] [--running] [--stopped]
```

### `vm fleet status`
Show status for instances across providers.
```bash
vm fleet status [--provider <name>] [--pattern <glob>] [--running] [--stopped]
```

### `vm fleet exec`
Run a command across instances.
```bash
vm fleet exec [--provider <name>] [--pattern <glob>] -- <command>
```

### `vm fleet copy`
Copy files to/from instances.
```bash
vm fleet copy [--provider <name>] [--pattern <glob>] <source> <dest>
```

### `vm fleet start`
Start instances.
```bash
vm fleet start [--provider <name>] [--pattern <glob>]
```

### `vm fleet stop`
Stop instances.
```bash
vm fleet stop [--provider <name>] [--pattern <glob>]
```

### `vm fleet restart`
Restart instances.
```bash
vm fleet restart [--provider <name>] [--pattern <glob>]
```

**View logs for specific service**:
```bash
vm logs --service postgresql
vm logs --service redis -f          # Follow Redis logs
vm logs -s mongodb --tail 200       # Last 200 lines of MongoDB
```

**Available services**: `postgresql`, `redis`, `mongodb`, `mysql`

**Press Ctrl+C** to stop following logs.

### `vm copy`
Copy files to/from a VM.
```bash
# Copy file to VM
vm copy /local/file.txt /remote/path/file.txt

# Copy file from VM (prefix with container name)
vm copy my-vm:/remote/file.txt /local/file.txt

# Auto-detect container in project directory
vm copy ./local.txt /workspace/remote.txt
```

## Configuration (`vm config`)
Manage `vm.yaml` configuration.

### `vm config validate`
Validate the current configuration.
```bash
vm config validate
```

### `vm config show`
Show the loaded configuration and its source.
```bash
vm config show
```

### `vm config set`
Set a configuration value.
```bash
vm config set <key> <value>
```

### `vm config get`
Get a configuration value.
```bash
vm config get [key]
```

### `vm config unset`
Remove a configuration field.
```bash
vm config unset <key>
```

### `vm config preset`
Apply a configuration preset.
```bash
vm config preset <name>
```

### `vm config profile`
List or set the default profile.
```bash
vm config profile list
vm config profile set <name>
```

### `vm config ports`
Manage port configuration and resolve conflicts.
```bash
vm config ports --fix
```

### `vm config clear`
Reset your configuration.
```bash
vm config clear
```

---

## Port Management

Ports are summarized in `vm status <vm>`.

### `vm tunnel create`
Create a dynamic port forwarding tunnel without permanent configuration.
```bash
vm tunnel create <mapping> [container]
```

**Forward localhost port to container port:**
```bash
vm tunnel create 8080:3000
```

Makes container port 3000 accessible at localhost:8080 on your host machine.

**Forward to specific container:**
```bash
vm tunnel create 9229:9229 myapp-dev
```

Useful for debugging or temporary access to services.

### `vm tunnel list`
List all active port forwarding tunnels.
```bash
vm tunnel list [container]
```

Shows which ports are currently being forwarded and to which containers.

### `vm tunnel stop`
Stop port forwarding tunnel(s).
```bash
vm tunnel stop [port] [container] [--all]
```

**Stop specific port:**
```bash
vm tunnel stop 8080
```

**Stop all tunnels:**
```bash
vm tunnel stop --all
```

**Use cases:**
- Debugging: Forward debugger port temporarily (`vm tunnel create 9229:9229`)
- Testing: Access internal service without permanent port config
- Conflict resolution: Tunnel to alternate host port when default is busy
- Temporary access: Forward database port for one-time query

See [Dynamic Port Forwarding](configuration.md#dynamic-port-forwarding) in configuration guide for detailed examples.

---

## Environment Variables
Manage environment variables via your `.env` file and application tooling.

---

## Temporary VMs (`vm temp`)
Work with ephemeral environments.

### `vm temp create`
Create a temporary VM with mounted folders.
```bash
vm temp create <folders...>
```

### `vm temp ssh`
Connect to the temporary VM.
```bash
vm temp ssh
```

### `vm temp status`
Show the temporary VM's status.
```bash
vm temp status
```

### `vm temp destroy`
Destroy the temporary VM.
```bash
vm temp destroy
```

### `vm temp mount`
Add a mount to a running temporary VM.
```bash
vm temp mount <path>
```

### `vm temp unmount`
Remove a mount from a temporary VM.
```bash
vm temp unmount <path>
```

### `vm temp mounts`
List the current mounts.
```bash
vm temp mounts
```

### `vm temp list`
List all temporary VMs.
```bash
vm temp list
```

---

## Plugins (`vm plugin`)
Extend `vm` with custom functionality.

### `vm plugin list`
List installed plugins.
```bash
vm plugin list
```

### `vm plugin info`
Show plugin details.
```bash
vm plugin info <name>
```

### `vm plugin install`
Install a plugin from a directory.
```bash
vm plugin install <path>
```

### `vm plugin remove`
Remove an installed plugin.
```bash
vm plugin remove <name>
```

### `vm plugin new`
Create a new plugin template.
```bash
vm plugin new <name>
```

### `vm plugin validate`
Validate a plugin's configuration.
```bash
vm plugin validate <name>
```

---

## Snapshots (`vm snapshot`)
Create, manage, and share VM snapshots.

### `vm snapshot create`
Create a new snapshot of current VM state.
```bash
vm snapshot create <name> [--project <name>] [--quiesce]
                          [--from-dockerfile <path>]
```

**Options:**
- `--quiesce`: Stop services before snapshotting for data consistency
- `--from-dockerfile <path>`: Build snapshot directly from a Dockerfile
- `--build-context <path>`: Directory context for Dockerfile build (default: ".")
- `--build-arg <key=value>`: Build arguments for Dockerfile

### `vm snapshot list`
List all available snapshots.
```bash
vm snapshot list [--project <name>] [--type <base|project>]
```

**Options:**
- `--type <base|project>`: Filter by snapshot type (base images or project states)

### `vm snapshot restore`
Restore VM to a snapshot.
```bash
vm snapshot restore <name> [--project <name>]
```

### `vm snapshot delete`
Delete a snapshot.
```bash
vm snapshot delete <name> [--project <name>] [--force]
```

### `vm snapshot export`
Export a snapshot to a portable file for sharing.
```bash
vm snapshot export <name> [--output <path>] [--compress <level>]
```

**Basic export:**
```bash
vm snapshot export my-snapshot
```

Creates `my-snapshot.snapshot.tar.gz` in current directory.

**Custom output location:**
```bash
vm snapshot export my-snapshot --output ~/backups/snapshot.tar.gz
```

**Adjust compression (1-9):**
```bash
vm snapshot export my-snapshot --compress 9
```

Higher compression = smaller file but slower. Default is 6.

### `vm snapshot import`
Import a snapshot from a file.
```bash
vm snapshot import <file> [--name <name>] [--verify] [--force]
```

**Basic import:**
```bash
vm snapshot import my-snapshot.snapshot.tar.gz
```

Uses the snapshot name embedded in the file.

**Override snapshot name:**
```bash
vm snapshot import backup.tar.gz --name team-baseline
```

**Verify checksum:**
```bash
vm snapshot import backup.tar.gz --verify
```

**Overwrite existing:**
```bash
vm snapshot import backup.tar.gz --force
```

**Use cases:**
- Team onboarding: Share baseline development environment
- Backup: Export snapshots before major changes
- Migration: Move snapshots between machines
- Distribution: Share pre-configured environments

---
Global snapshots use the `@name` convention (for example, `vm snapshot create @vibe-base`).

---

## Database (`vm db`)
Manage databases and backups.

### `vm db list`
List all databases with sizes and backup counts.
```bash
vm db list
```

**Output:**
```
Databases:
  - myapp_dev                     125MB (3 backups)
  - test_db                       45MB (1 backup)
  - postgres                      8MB (no backups)

Backups stored in: ~/.vm/backups/postgres/
```

### `vm db backup`
Create a backup of a database.

**Backup single database:**
```bash
vm db backup <db_name>

# Example
vm db backup myapp_dev
```

**Backup all databases:**
```bash
vm db backup --all

# Excludes system databases (postgres, template0, template1)
```

**Backups are stored in:** `~/.vm/backups/postgres/` by default
**Format:** PostgreSQL custom format (`.dump`)
**Retention:** Keeps last 5 backups by default (configurable in `~/.vm/config.yaml`)

### `vm db restore`
Restore a database from a backup.
```bash
vm db restore <backup_name> <db_name>

# Example
vm db restore myapp_dev_20250127_143022.dump myapp_dev
```

**⚠️ Warning:** This will **drop and recreate** the database before restoring.

### `vm db export`
Export a database to a SQL file (text format).
```bash
vm db export <db_name> <file_path>

# Example
vm db export myapp_dev ./backup.sql
```

**Use export when:**
- Sharing with others (readable SQL)
- Version controlling schema
- Cross-platform compatibility

### `vm db import`
Import a database from a SQL file.
```bash
vm db import <file_path> <db_name>

# Example
vm db import ./backup.sql myapp_dev
```

### `vm db size`
Show disk usage per database.
```bash
vm db size
```

### `vm db reset`
Drop and recreate a database (delete all data).
```bash
vm db reset <db_name>

# Skip confirmation
vm db reset <db_name> --force
```

### `vm db credentials`
Show the generated credentials for a database service.
```bash
vm db credentials <service_name>

# Example
vm db credentials postgresql
```

---

### Backup vs Export

| Feature | Backup | Export |
|---------|--------|--------|
| Format | Binary (.dump) | Text (.sql) |
| Size | Compressed, smaller | Larger |
| Speed | Faster | Slower |
| Retention | Auto-managed | Manual |
| Location | `~/.vm/backups/postgres/` | User-specified |
| Use case | Regular backups, snapshots | Sharing, version control |

**Recommendation:** Use `backup` for regular backups, use `export` when you need readable SQL files.

---

## Secrets Management (`vm auth`)
Manage secrets and credentials.

### `vm auth add`
Store a secret.
```bash
vm auth add <name> <value>
```

### `vm auth list`
List stored secrets.
```bash
vm auth list
```

### `vm auth remove`
Remove a secret.
```bash
vm auth remove <name>
```

---

## Package Registry (`vm pkg`)
Manage private package registries.

### `vm pkg add`
Publish a package from the current directory.
```bash
vm pkg add [--type <type>]
```

### `vm pkg list`
List all packages in the registry.
```bash
vm pkg list
```

### `vm pkg remove`
Remove a package from the registry.
```bash
vm pkg remove <name>
```

---

## System Management

### `vm doctor`
Run comprehensive health checks.
```bash
vm doctor [--fix] [--clean]
```

### `vm update`
Update `vm` to the latest or a specific version.
```bash
vm update [--version <version>]
```

### `vm uninstall`
Uninstall `vm` from your system.
```bash
vm uninstall
```
