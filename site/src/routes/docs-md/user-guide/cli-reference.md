# CLI Reference

All `vm` commands with usage examples and expected output. Use this as a reference when building workflows or troubleshooting issues.

## Quick Reference

| Task | Command |
|------|---------|
| Create VM | `vm create` |
| Start VM | `vm start` |
| Stop VM | `vm stop` |
| Connect to VM | `vm ssh` |
| Check status | `vm status` |
| View logs | `vm logs [-f]` |
| Wait for services | `vm wait [--service <name>]` |
| Destroy VM | `vm destroy` |
| List VMs | `vm list` |
| Run command | `vm exec <command>` |
| **Snapshots** | |
| Create snapshot | `vm snapshot create <name>` |
| Restore snapshot | `vm snapshot restore <name>` |
| Export snapshot | `vm snapshot export <name>` |
| Import snapshot | `vm snapshot import <file>` |
| **Configuration** | |
| Initialize config | `vm init` |
| Validate config | `vm config validate` |
| Apply preset | `vm config preset <name>` |
| **Environment** | |
| Validate .env | `vm env validate` |
| Show env diff | `vm env diff` |
| **Port Management** | |
| Show ports | `vm ports` |
| Forward port | `vm port forward <host>:<container>` |
| List tunnels | `vm port list` |
| Stop tunnel | `vm port stop [port]` |
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
- [Configuration (`vm config`)](#configuration-vm-config)
- [Port Management](#port-management)
- [Environment Variables (`vm env`)](#environment-variables-vm-env)
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

### `vm create`
Create and apply a new VM.
```bash
vm create [--force]
```

### `vm start`
Start a stopped VM.
```bash
vm start
```

### `vm stop`
Stop a running VM.
```bash
vm stop
```

### `vm restart`
Restart a VM.
```bash
vm restart
```

### `vm apply`
Re-run the applying process on an existing VM.
```bash
vm apply
```

### `vm destroy`
Destroy a VM and all its associated resources.
```bash
vm destroy [--no-backup] [--force]
```

### `vm status`
Show the status and health of a VM.
```bash
vm status
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

### `vm list`
List all available VMs.
```bash
vm list
```

### `vm wait`
Wait for services to become ready before proceeding.
```bash
vm wait [--service <name>] [--timeout <seconds>]
```

**Wait for all services:**
```bash
vm wait
```

**Wait for specific service:**
```bash
vm wait --service postgresql
vm wait --service redis --timeout 60
```

**Options:**
- `--service <name>` - Wait for specific service (postgresql, redis, mongodb, mysql)
- `--timeout <seconds>` - Maximum wait time in seconds (default: 120)
- `--container <name>` - Target specific container

**Use cases:**
- CI/CD pipelines: Wait for database before running migrations
- Scripts: Ensure services are ready before executing commands
- Development: Verify services started correctly after `vm create`

---

## Configuration (`vm config`)
Manage `vm.yaml` configuration.

### `vm init`
Initialize a new `vm.yaml` configuration file.
```bash
vm init
```

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

### `vm ports`
Show all listening ports and services in the VM.
```bash
vm ports [container]
```

Displays which services are listening on which ports inside the container.

**Example output:**
```
Port   Service        Status
3000   frontend       listening
5432   postgresql     listening
6379   redis          listening
```

### `vm port forward`
Create dynamic port forwarding tunnel without permanent configuration.
```bash
vm port forward <mapping> [container]
```

**Forward localhost port to container port:**
```bash
vm port forward 8080:3000
```

Makes container port 3000 accessible at localhost:8080 on your host machine.

**Forward to specific container:**
```bash
vm port forward 9229:9229 myapp-dev
```

Useful for debugging or temporary access to services.

### `vm port list`
List all active port forwarding tunnels.
```bash
vm port list [container]
```

Shows which ports are currently being forwarded and to which containers.

### `vm port stop`
Stop port forwarding tunnel(s).
```bash
vm port stop [port] [container] [--all]
```

**Stop specific port:**
```bash
vm port stop 8080
```

**Stop all tunnels:**
```bash
vm port stop --all
```

**Use cases:**
- Debugging: Forward debugger port temporarily (`vm port forward 9229:9229`)
- Testing: Access internal service without permanent port config
- Conflict resolution: Tunnel to alternate host port when default is busy
- Temporary access: Forward database port for one-time query

See [Dynamic Port Forwarding](configuration.md#dynamic-port-forwarding) in configuration guide for detailed examples.

---

## Environment Variables (`vm env`)
Manage environment variables and validate against templates.

Requires `project.env_template_path` configured in vm.yaml.

### `vm env validate`
Validate .env file against template.
```bash
vm env validate [--all]
```

**Basic validation:**
```bash
vm env validate
```

Shows missing variables that need to be set.

**Show all variables:**
```bash
vm env validate --all
```

Shows missing, extra, and present variables.

### `vm env diff`
Show differences between .env and template.
```bash
vm env diff
```

Displays side-by-side comparison of template vs actual values.

### `vm env list`
List all environment variables from .env.
```bash
vm env list [--show-values]
```

**List variable names:**
```bash
vm env list
```

**Show values (masked):**
```bash
vm env list --show-values
```

**Use cases:**
- Onboarding: Verify team members have all required environment variables
- CI/CD: Validate environment configuration before deployment
- Development: Check which environment variables are currently configured

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
vm snapshot create <name> [--project <name>]
```

### `vm snapshot list`
List all available snapshots.
```bash
vm snapshot list [--project <name>]
```

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
vm doctor
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