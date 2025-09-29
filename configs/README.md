# VM Configuration Files

This directory contains configuration files that are **embedded into the VM binary** at compile time. These are NOT user-facing examples.

## Directory Structure

### `presets/`
Framework and language presets that users can apply via `vm config preset <name>`.

**Embedded in**: `vm-config/src/embedded_presets.rs`

Available presets:
- `base` - Minimal base configuration
- `nodejs`, `python`, `rust` - Language-specific environments
- `react`, `next`, `django`, `rails` - Framework-specific setups
- `docker`, `kubernetes` - Container orchestration presets
- `tart-*` - Platform-specific virtualization presets

### `languages/`
Language-specific package manager configurations (npm, pip, cargo).

### `os_defaults/`
Operating system-specific default configurations.

### `services/`
Service definition templates (PostgreSQL, Redis, MongoDB, Docker).

### `schema/`
JSON Schema definitions for configuration validation.

## User-Facing Examples

For user-facing configuration examples, see the `/examples/` directory in the project root.

## Relationship to Examples

| Directory | Purpose | Audience | Embedded |
|-----------|---------|----------|----------|
| `configs/` | Production templates with all options | Developers | ✅ Yes |
| `examples/` | Simplified user documentation | End users | ❌ No |

## Modifying Configurations

When modifying files in this directory:
1. Update the corresponding file in `configs/`
2. Rebuild the project: `cd rust && cargo build`
3. The changes will be embedded in the next binary
4. Consider updating user examples in `/examples/` if relevant

## Schema Validation

All configurations are validated against schemas in `configs/schema/`. Use `vm validate` to check configuration files.