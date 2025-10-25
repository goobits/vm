# VM Snapshots - The New Way to Share Environments

**This directory previously contained example configurations for manual VM setup.**

**As of v3.2.0, the recommended approach is to use VM snapshots instead.**

## Why Snapshots?

The old workflow:
- ❌ Copy example configs manually
- ❌ Lose state when destroying VMs
- ❌ Can't capture installed packages or code changes
- ❌ Requires rebuilding from scratch

The new workflow:
- ✅ `vm snapshot create my-setup` - Captures everything
- ✅ `vm snapshot restore my-setup` - Instant environment recovery
- ✅ Share complete working environments with teammates
- ✅ Safe experimentation with easy rollback

## Migration Guide

### Old Way (Deprecated)
```bash
cp examples/nextjs-app/vm.yaml ./
vm init
vm create
# Manually configure packages, databases, etc.
```

### New Way (Recommended)
```bash
# Set up once
vm init
vm create
# Install packages, configure services, etc.

# Save your working environment
vm snapshot create my-setup --description "Fully configured Next.js environment"

# Later: instant restoration
vm snapshot restore my-setup
```

## For Template Configs

If you need starter `vm.yaml` files, use:
```bash
vm init  # Interactive configuration wizard
```

Or see [Configuration Guide](../docs/user-guide/configuration.md) for examples.

## Documentation

- [Snapshot User Guide](../docs/user-guide/configuration.md#vm-snapshots-)
- [Configuration Reference](../docs/user-guide/configuration.md)
