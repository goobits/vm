# 🛠️ VM CLI Reference

This document provides a complete reference for all `vm` commands.

## 📚 Table of Contents
- [Global Options](#global-options)
- [Core Commands](#core-commands)
- [Configuration (`vm config`)](#configuration-vm-config)
- [Temporary VMs (`vm temp`)](#temporary-vms-vm-temp)
- [Plugins (`vm plugin`)](#plugins-vm-plugin)
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
View the logs for a VM.
```bash
vm logs
```

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

## Database (`vm db`)
Manage databases and backups.

### `vm db backup`
Create a backup of a database.
```bash
vm db backup <db_name>
```

### `vm db restore`
Restore a database from a backup.
```bash
vm db restore <backup_name> <db_name>
```

### `vm db credentials`
Show the generated credentials for a database service.
```bash
vm db credentials <service_name>
```

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