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
- `rust/vm-packages/` owns package identities, resolver policy, client
  environment, and shared workflow contracts.
- `rust/vm-package-server/` owns native npm, Cargo, and Python protocol adapters
  plus the worker-local read-only cache/proxy edge.
- `rust/vm-package-work/` owns durable checkout, lease, submission, integration,
  rollout, bundle, and receipt state. Editable isolated source belongs to the
  authenticated managed guest; workflow services retain immutable bundles and
  only create transient internal processing trees.
- `rust/vm-package-jobs/` owns persistent review, credential-separated binary
  build, release, and rollout workers plus isolated tool publication inside
  infrastructure containers.
- `configs/` owns embedded configuration; `examples/` must not be treated as
  runtime defaults.

Command modules should not duplicate provider behavior. Providers should not
own user interaction or top-level command routing.

Package control and data planes remain separate. The central appliance owns
mutable workflow state and immutable releases. Each worker edge exposes native
package protocols, delegates all source selection to the shared resolver, and
holds only read credentials and persistent read-through cache. Development
overrides use explicit checkout-scoped package-manager configuration; they
never make one published name/version return different bytes.

Package infrastructure never launches an agent. The guest CLI derives checkout
ownership from its signed consumer capability, while review and release workers
consume authenticated immutable bundles rather than a shared editable checkout.
The workflow service also derives source-only package status from registered
consumer usage, so submission and integration enforce the same validation
scope. Guest cancellation is two-phase: restore local dependency state first,
then use the consumer-bound capability to close durable checkout state.

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
