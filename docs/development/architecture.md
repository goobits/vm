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
- `rust/vm/src/commands/` owns one exhaustive command dispatcher. Individual
  command modules own their preparation, and `dry_run` owns dry-run wording.
- `rust/vm-config/` is a library that owns configuration loading, validation,
  profiles, schema behavior, and the single preset-to-project initialization path. Preset command
  IO is separate from private preset resolution/materialization.
- `rust/vm-plugin/` owns plugin discovery and the validation facade; metadata,
  preset-content, and service-content rules remain separate private concerns.
- `rust/vm-provider/` owns Docker, Podman, and Tart implementation. Its factory
  aggregate composes command, instance-lifecycle, and provisioning capabilities;
  temporary-VM behavior is an explicit optional capability.
- `rust/vm-temp/` owns temporary lifecycle orchestration, state, status, and
  mount mutation behind the `TempVmOps` facade.
- `rust/vm-snapshot/` owns snapshot creation, restoration, import, and export.
  Archive safety/staging, images, volumes, and Dockerfile base images have one
  private owner each.
- `rust/vm-platform/` owns platform-specific paths, process integration, and
  host CPU/memory detection.
- `rust/vm-core/` owns shared filesystem, command, prompt, and message-format
  utilities plus system requirement policy; it consumes platform facts rather
  than detecting them.
- `rust/vm-packages/` owns package identities, resolver policy, client
  environment, and shared workflow contracts. One public infrastructure client
  delegates privately to endpoint-domain implementations and one transport.
- `rust/vm-package-server/` owns native npm, Cargo, and Python protocol adapters
  plus the worker-local read-only cache/proxy edge. Protocol modules own their
  validation; server setup and routing remain separate internal concerns. Its
  external API is the small crate-root facade rather than those module paths.
- `rust/vm-package-work/` owns durable checkout, lease, submission, integration,
  rollout, bundle, and receipt state. Editable isolated source belongs to the
  authenticated managed guest; workflow services retain immutable bundles and
  only create transient internal processing trees.
- `rust/vm-package-jobs/` owns persistent review, credential-separated binary
  build, release, and rollout workers plus isolated tool publication inside
  infrastructure containers. Release source/workflow coordination is separate
  from tool archives, artifact assembly, isolated builds, and publication.
- `configs/` owns embedded configuration; `examples/` must not be treated as
  runtime defaults.

Command modules should not duplicate provider behavior. Providers should not
own user interaction or top-level command routing.

The package work-session router resolves one package or tool identity to an
exact controller-attested Git root and that root's owning writable Docker
configuration. It then delegates the interactive connection to the existing
provider shell lifecycle. It exposes no general environment, path, or command
targeting, creates no checkout or separate lease, and never falls back to the
isolated workflow. Guest-owned `packages checkout` remains the explicit copied
source path; both modes converge on the same review, integration, and release
services.

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

Binary and collection publication persists one immutable activation plan in
the workflow service. A provider-authorized host worker claims that plan and
updates globally enrolled running environments in place; stopped environments
remain deferred until their normal start path. The package appliance never
receives a Docker socket. Worker leases, target receipts, and idempotency keys
make controller or Docker interruption resumable without republishing.

## CLI Output

`rust/vm-core/src/output_macros.rs` owns shared output primitives. Requested
data and successful results use stdout; progress, warnings, hints, and errors
use stderr. Libraries return errors without printing them, and the `vm`
executable renders each fatal error once.

`rust/vm-logging/` owns tracing initialization and HTTP request context. CLI
logs remain separate from requested output; package services emit JSON to
stderr by default and correlate request spans and response headers with the
same bounded `x-request-id`. Command diagnostics name executables but omit
arguments because arguments can contain credentials.

`LOG_LEVEL`, `LOG_FORMAT=human|json|auto`, and
`LOG_OUTPUT=console|file|both` control the shared subscriber. `RUST_LOG` is the
advanced target-filter override, `LOG_TAGS=key:value` filters span context, and
`LOG_FILE_PATH` selects the rolling file base when file output is enabled. CLI
defaults are error-level human logs in the file sink; services default to
info-level JSON on stderr.

## Provider Boundaries

Callers borrow the narrowest capability they need: `CommandProvider` for guest
commands, `InstanceProvider` for lifecycle and discovery, and
`ProvisioningProvider` for mutable runtime reconciliation. `Provider` remains
the factory-owned aggregate used when orchestration genuinely spans capabilities.

Docker and Podman implement container mounts, named volumes, tmpfs, resource
limits, and logging. Tart owns macOS/Linux guest provisioning and does not
accept container-only storage settings.

Compose rendering/writing is separate from container execution and package-edge
reconciliation. Tart combines a package-infrastructure batch with a project-runtime
batch rather than splitting provisioning by language.

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
