# Migration Guide

This document helps users migrate between versions of Goobits VM and understand breaking changes.

## Current Version - Security Configuration

### 🔒 Security Defaults

Containers use secure defaults for host protection.

**Current defaults:**
- `SYS_PTRACE` capability **disabled** (debugging tools disabled by default)
- `seccomp` filtering **enabled** (blocks dangerous syscalls)
- `no-new-privileges` **enabled** (blocks SUID escalation)

### Migration Steps

**Most users:** No action needed - containers are more secure and still functional.

**Users needing debugging tools:** Add to your `vm.yaml`:

```yaml
# Enable debugging tools (reduces security)
security:
  enable_debugging: true     # Enables SYS_PTRACE and seccomp=unconfined
  no_new_privileges: false   # Allows SUID operations
```

### Rationale

1. **Host escape prevention** - Secure defaults reduce attack surface
2. **Container isolation** - Tool provides secure execution environment
3. **Industry best practice** - Security should be default, convenience opt-in
4. **Performance** - Seccomp filtering and no ptrace reduces overhead

---

## v1.3 - OS Field and Tart Provider

### Added
- **Simple OS Configuration**: New `os` field for simplified setup
  - `os: ubuntu` → Docker provider, 4GB RAM
  - `os: macos` → Tart provider, 8GB RAM
  - `os: debian` → Docker provider, 2GB RAM
  - `os: alpine` → Docker provider, 1GB RAM

- **Tart Provider**: Native Apple Silicon virtualization
  - ARM64 performance with x86 emulation support
  - SSH and folder sharing configured automatically

### Migration
- Existing `provider:` configurations continue to work
- New projects can use simplified `os:` field

---

## v1.2 - Rust Migration

### Major Changes
- **Core migrated to Rust** for performance and type safety
- **Structured logging** with context tracking
- **Updated dependencies** to current stable versions

### Migration
- No configuration changes needed
- Improved performance and error messages
- Better type checking and validation

---

## v1.1 - Smart Preset System

### Added
- **Zero-config startup** with automatic project detection
- **Multi-technology support** (React + Django, etc.)
- **Preset commands**: `vm preset list`, `vm preset show`
- **Preset management**: `vm config preset <name>`

### Migration
- Existing configurations work unchanged
- New projects benefit from automatic detection
- Use `--no-preset` to disable if needed

---

## v1.0 - Initial Release

### Core Features
- Docker and Vagrant provider support
- YAML configuration system
- Temporary VM functionality
- Configuration migration tools

---

## Common Migration Patterns

### From Shell Scripts to Rust Binary
If upgrading from very old versions:

1. **Remove old shell binary**: `rm ~/.local/bin/vm`
2. **Install new version**: `./install.sh`
3. **Test configuration**: `vm validate`

### Configuration Format Changes

#### JSON to YAML (v1.2+)
```bash
# Old: vm.json
{
  "provider": "docker",
  "ports": {"app": 3000}
}

# New: vm.yaml
provider: docker
ports:
  app: 3000
```

#### Provider to OS Field (v1.3+)
```bash
# Old explicit provider
provider: docker
vm:
  memory: 4096

# New simplified OS
os: ubuntu  # Automatically sets Docker + 4GB
```

### Port Conflicts (v1.2+)
If you see port 3150 conflicts:
- Update to v1.2+ (fixed VM tool port conflicts)
- Check `vm.yaml` doesn't hardcode conflicting ports

### Temporary VMs (v1.2+)
New modular architecture:
```bash
# Old monolithic approach
vm temp create ./src

# New modular commands
vm temp create ./src
vm temp ssh
vm temp destroy
```

---

## Troubleshooting Migration Issues

### Debugging Tools Broken (v2.0+)
```yaml
# Add to vm.yaml if you need gdb, strace, etc.
security:
  enable_debugging: true
```

### Permission Errors (v2.0+)
```yaml
# If SUID tools needed
security:
  no_new_privileges: false
```

### Configuration Not Found
```bash
# Check config search path
vm config get

# Create new config
vm init
```

### Provider Issues
```bash
# Verify provider availability
docker --version  # For Docker provider
vagrant --version # For Vagrant provider
tart --version    # For Tart provider (macOS)
```

---

## Questions?

- **Security**: See [Security Documentation](docs/user-guide/security.md)
- **Configuration**: See [Configuration Guide](docs/user-guide/configuration.md)
- **Issues**: Report at [GitHub Issues](https://github.com/goobits/vm/issues)