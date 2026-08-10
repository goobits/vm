# VM Architecture

This document describes repository-level ownership. The canonical Rust crate
map and dependency details live in
[rust/ARCHITECTURE.md](../../rust/ARCHITECTURE.md).

## Repository Map

```text
vm/
|-- configs/     Embedded schemas, defaults, services, and presets
|-- docs/        User and contributor documentation
|-- examples/    Small user-facing examples
|-- plugins/     Optional VM extensions
`-- rust/        Rust workspace and the `vm` binary
```

## Ownership Boundaries

- `rust/vm/src/cli/` owns command parsing and the public command shape.
- `rust/vm/src/commands/` owns application orchestration.
- `rust/vm-config/` owns configuration loading, validation, profiles, and
  schema behavior.
- `rust/vm-provider/` owns Docker, Podman, and Tart lifecycle implementation.
- `rust/vm-snapshot/` owns snapshot creation, restoration, import, and export.
- `rust/vm-core/` owns shared filesystem, command, prompt, and message-format
  utilities.
- `configs/` owns embedded configuration; `examples/` must not be treated as
  runtime defaults.

Command modules should not duplicate provider behavior. Providers should not
own user interaction or top-level command routing.

## CLI Output

`rust/vm-core/src/output_macros.rs` owns shared output primitives. Requested
data and successful results use stdout; progress, warnings, hints, and errors
use stderr. Libraries return errors without printing them, and the `vm`
executable renders each fatal error once.

## Provider Boundaries

Docker and Podman implement container mounts, named volumes, tmpfs, resource
limits, and logging. Tart owns macOS/Linux guest provisioning and does not
accept container-only storage settings.

Host-side project detection produces one provider-neutral install plan. Docker
Ansible provisioning and Tart guest provisioning consume that plan and the
same embedded Node, AI-tool, home-repair, shell, and cache policies rather than
probing or implementing them independently.

Provider-independent config validation runs before lifecycle operations.
`vm config render` is a redacted, provider-free preview and must remain safe to
run without Docker, Podman, or Tart.

## Dependency Direction

Dependencies flow from foundation crates through configuration and providers
to the `vm` application. Shared behavior belongs in the lowest existing owner
that can provide it without creating a cycle.

Plugin-backed workflows remain top-level user commands while their discovery
and metadata live in `vm-plugin`.

## Further Reading

See the [Development Guide](guide.md), [Testing Guide](testing.md), and
[Rust Architecture](../../rust/ARCHITECTURE.md). The testing guide owns the
supported quality commands.
