# Security Configuration

## Overview

Goobits VM provides configurable security options to balance development convenience with container isolation. By default, the system prioritizes developer productivity, but you can enable additional security features to prevent container escape and enhance isolation.

## Host Escape Prevention

The following settings directly affect the container's ability to escape to the host system:

### 1. Debugging Capabilities (Default: Disabled for Security)

By default, containers have debugging capabilities disabled for maximum host protection:

```yaml
# vm.yaml - Enable debugging tools (reduces security)
security:
  enable_debugging: true   # Adds SYS_PTRACE and disables seccomp filtering
```

**Impact of enabling debugging:**
- ✅ Allows debuggers (gdb, strace, ltrace) to work
- ✅ Some development tools work better
- ❌ Enables ptrace-based attacks on host processes
- ❌ Disables syscall filtering protection

### 2. No New Privileges (Default: Enabled for Security)

By default, privilege escalation through SUID binaries is blocked:

```yaml
# vm.yaml - Disable privilege blocking (reduces security)
security:
  no_new_privileges: false  # Allows SUID/SGID privilege escalation
```

**Impact of disabling:**
- ✅ Some system administration tools work better
- ❌ Allows privilege escalation attacks
- ❌ Enables exploitation of SUID binaries

### 3. User Namespace Remapping (Default: Disabled)

Remap container UIDs to unprivileged host UIDs:

```yaml
security:
  user_namespaces: true  # Container root ≠ host root
```

**Impact:**
- ✅ Container root becomes unprivileged on host
- ✅ Significantly reduces impact of container escape
- ❌ May cause permission issues with mounted volumes
- ❌ Requires Docker daemon configuration

## Container Hardening

Additional security options that make the container harder to compromise:

### Resource Limits

Prevent resource exhaustion attacks:

```yaml
security:
  memory_limit: "2g"      # Maximum memory usage
  cpu_limit: "2.0"        # Maximum CPU cores
  pids_limit: 200         # Maximum process count
```

### Read-Only Root Filesystem

Make the root filesystem immutable:

```yaml
security:
  read_only_root: true    # Root filesystem becomes read-only
```

**Note:** This requires explicit tmpfs mounts for writable directories.

### Drop Capabilities

Remove specific Linux capabilities:

```yaml
security:
  drop_capabilities:
    - NET_ADMIN
    - SYS_TIME
    - SYS_MODULE
```

### Custom Security Options

Add additional Docker security flags:

```yaml
security:
  security_opts:
    - apparmor=docker-default
    - label=type:container_t
```

## Security Profiles

### Secure Profile (Default)

Good balance of security and functionality for most use cases:

```yaml
# No security section needed - defaults provide:
# - enable_debugging: false (no ptrace/seccomp vulnerabilities)
# - no_new_privileges: true (blocks SUID escalation)
# - user_namespaces: false (for file permission compatibility)
```

### Development Profile

Maximum convenience when you need debugging tools:

```yaml
security:
  enable_debugging: true     # Enable gdb, strace, etc.
  no_new_privileges: false   # Allow system administration tools
```

### Hardened Profile

Maximum security for untrusted code:

```yaml
security:
  enable_debugging: false    # Already default
  no_new_privileges: true    # Already default
  user_namespaces: true      # Add UID remapping
  read_only_root: true       # Immutable filesystem
  memory_limit: "2g"
  cpu_limit: "2.0"
  pids_limit: 200
  drop_capabilities:
    - NET_ADMIN
    - SYS_TIME
    - SYS_MODULE
    - SYS_ADMIN
```

## Path Security

The VM system includes built-in path validation that prevents mounting dangerous system directories:

**Blocked mount paths:**
- `/` (root)
- `/etc` (system configuration)
- `/usr` (system binaries)
- `/var` (system state)
- `/bin`, `/sbin` (system executables)
- `/boot` (kernel/bootloader)
- `/sys`, `/proc`, `/dev` (kernel interfaces)
- `/root` (root home)

These restrictions apply to both regular and temporary VMs.

## Best Practices

1. **For most users:** Default settings provide good security while maintaining functionality
2. **For debugging needs:** Enable `security.enable_debugging: true` when using gdb/strace
3. **For untrusted code:** Use the hardened profile with user namespaces
4. **For AI agents:** Default settings are already secure (no debugging capabilities)
5. **Always:** Keep the VM tool updated for latest security patches

## Verification

Check your security settings:

```bash
# View current configuration
vm config get security

# Test security settings
docker inspect $(vm status --raw) | grep -A 10 SecurityOpt
```

## Troubleshooting

### Debugging tools not working

If gdb, strace, or similar tools fail:

```yaml
security:
  enable_debugging: true  # Re-enable debugging capabilities
```

### Permission denied errors

If you encounter permission issues with mounted files:

```yaml
security:
  user_namespaces: false  # Disable UID remapping
```

### Out of memory errors

Adjust resource limits:

```yaml
security:
  memory_limit: "8g"      # Increase memory limit
  pids_limit: 1000        # Increase process limit
```