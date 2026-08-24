# Goobits VM Architecture

## Overview

Goobits VM is built using a **layered architecture** designed around the principles of separation of concerns, dependency injection, and circular dependency elimination. The architecture promotes modularity, testability, and maintainability by organizing functionality into distinct crates with clear responsibilities and well-defined interfaces.

### Key Architectural Goals

- **Separation of Concerns**: Each crate has a single, well-defined responsibility
- **Dependency Flow Control**: Dependencies flow in one direction, preventing circular dependencies
- **Provider Abstraction**: Multiple VM providers (Docker, Podman, Tart) through a unified interface
- **Error Consistency**: Unified error handling across all components
- **Testability**: Modular design enables comprehensive testing at all levels

## Crate Quick Reference

| Layer | Crate | Primary Responsibility | Quick Checks |
| --- | --- | --- | --- |
| Foundation | `vm-platform` | OS integration, paths, and host resource detection | `cargo test -p vm-platform` |
| Foundation | `vm-core` | Shared errors, output primitives, FS and command utilities | `cargo test -p vm-core` |
| Foundation | `vm-messages` | Reusable config, plugin, and service message templates | `cargo test -p vm-messages` |
| Foundation | `vm-logging` | Tracing subscriber + log routing setup used by every binary | `cargo test -p vm-logging` |
| Configuration | `vm-config` | Configuration schema, detectors, CLI helpers | `cargo test -p vm-config` |
| Configuration | `vm-plugin` | Plugin discovery, validation, and preset/service loading | `cargo test -p vm-plugin` |
| Provider | `vm-provider` | Provider traits plus Docker/Podman/Tart implementations | `cargo test -p vm-provider` |
| Provider | `vm-temp` | Temporary VM lifecycle, mount management, CLI glue | `cargo test -p vm-temp` |
| Provider | `vm-snapshot` | Snapshot lifecycle: create, restore, export, and import | `cargo test -p vm-snapshot` |
| Application | `vm` | Main CLI orchestration and commands | `cargo test -p goobits-vm` / `cargo run -p goobits-vm -- --help` |
| Application | `vm-installer` | Self-installation flow for distributing the CLI | `cargo run -p vm-installer -- --help` |
| Domain | `vm-packages` | Package protocols, workflow records, appliance definition, client | `cargo test -p vm-packages` |
| Service | `vm-package-server` | Private registry protocols and worker read edge | `cargo test -p vm-package-server` |
| Service | `vm-package-work` | Deterministic checkout, review, release, and rollout state | `cargo test -p vm-package-work` |
| Service | `vm-package-jobs` | Persistent review, release, and rollout workers | `cargo test -p vm-package-jobs` |
| Service | `vm-auth-proxy` | Authentication proxy that fronts API/services | `cargo run -p vm-auth-proxy -- --help` |
| Tooling | `version-sync` | Keeps version numbers aligned across manifests | `cargo run -p version-sync -- check` |

## Crate Architecture

### Foundation Layer

#### vm-platform
**Role**: The sole owner of platform-specific operations and host resource detection.

**Key Exports**: Platform paths, shell detection, process integration, and resource probing

#### vm-core
**Role**: The foundational crate providing shared utilities and error handling for the entire workspace.

**Responsibilities**:
- Unified error types (`VmError`) used throughout the system
- Cross-cutting utilities (file system operations and command execution)
- Core traits and interfaces shared across crates
- System validation and health checks
- Message substitution and shared stdout/stderr primitives (`msg!`, `vm_println!`,
  `vm_progress!`, `vm_error!`, etc.)

**Key Exports**: `VmError`, `Result`, output macros, file system utilities,
command streaming, and system validation

#### vm-messages
**Role**: Pure data crate containing reusable domain message templates.

**Responsibilities**:
- Configuration workflow messages
- Plugin workflow messages
- Service and installer messages
- Zero dependencies on other workspace crates

**Key Exports**: `MESSAGES` constant with categorized message templates

General lifecycle copy stays with its command. Shared output behavior belongs
to `vm-core`; the executable renders each fatal error once.

### Configuration Layer

#### vm-config
**Role**: Configuration management, validation, and project detection capabilities.

**Responsibilities**:
- VM configuration schema definition and validation
- YAML configuration file parsing and generation
- Project type detection (Node.js, Python, Docker, etc.)
- Port management and allocation
- Global settings and user preferences
- Configuration-specific CLI helpers

**Key Exports**: `config::VmConfig`, `AppConfig`, `GlobalConfig`, project detectors,
and root CLI/config-operation facades

Preset command validation and file IO stay in `config_ops::preset`; declared-preset
resolution and minimal project-config materialization live in its private
`materialize` module.
The embedded YAML schemas are the canonical field/type registry used by both
editor validation and schema-aware `vm config set` value parsing.
Implementation modules for loading, merging, presets, schema lookup, paths, and
YAML operations are private; stable domain entry points are exported from the
crate root while configuration types, detectors, ports, and validation retain
their explicit namespaces.

#### vm-plugin
**Role**: Plugin discovery, loading, and validation.

The validation facade owns result types and cross-variant rules. Metadata,
preset-content, and service-content validation are separate private owners.

### Provider Layer

#### vm-provider
**Role**: Provider abstraction layer enabling support for multiple VM technologies.

**Responsibilities**:
- `CommandProvider`, `InstanceProvider`, and `ProvisioningProvider` capability
  traits for command transport, lifecycle/state, and runtime reconciliation
- `Provider` as the factory-owned aggregate over those capabilities, not a
  second owner of their methods
- `TempProvider` capability trait for optional temporary VM operations
- Docker, Podman, and Tart provider implementations
- VM lifecycle management (create, start, stop, destroy)
- Enhanced status reporting with real-time metrics
- Service health monitoring and port mapping

**Key Exports**: Provider capability traits, `get_provider()` factory, provider
implementations

Container Compose code owns rendering and secure writes only. Lifecycle execution
owns start/stop behavior; package-edge reconciliation and pipx classification have
separate lifecycle owners. Tart provisioning combines package infrastructure and
project-runtime command batches without duplicating the shared `ProjectPlan`.

#### vm-temp
**Role**: Temporary VM management for ephemeral development environments.

**Responsibilities**:
- Temporary VM lifecycle management
- Dynamic mount point management
- Cleanup and resource management
- Integration with main VM providers

**Key Exports**: `TempVmOps`, `StateManager`, `TempVmState`, `MountPermission`

#### vm-snapshot
**Role**: Snapshot lifecycle management for VM state, used both for project-scoped
checkpoints and for global base-image snapshots.

**Responsibilities**:
- Create, restore, list, and delete snapshots scoped per project or globally
- Export snapshots to a portable `.tar.gz` archive (image layers + volumes + metadata)
- Import snapshots, validating manifest, platform compatibility, and entry paths
  before extraction
- Parallel Docker image save/load for large multi-service snapshots

Archive safety/staging, image operations, volume operations, and Dockerfile base
images are separate private owners behind these entry points.

**Key Exports**: `SnapshotManager`, `SnapshotScope`, `SnapshotMetadata`,
import/export entry points

### Application Layer

#### vm
**Role**: Main CLI application binary that orchestrates all other components.

**Responsibilities**:
- CLI command implementation and routing
- User interaction and experience
- Command validation and execution
- Integration of all lower-level components
- Service registration and management
- Provider-authorized host worker for durable managed-tool activation plans

Guest shell programs live as adjacent `.sh` resources and are embedded with
`include_str!`; Rust modules own orchestration and typed parsing, not large
inline program bodies.

**Key Exports**: Main binary, command handlers, service orchestration

The root command module retains one exhaustive dispatcher. Command-specific
preparation stays with each command module, and dry-run descriptions have one
private policy owner.

### Service Layer

#### vm-packages
**Role**: Provider-neutral package-infrastructure contract and client.

**Responsibilities**:
- Shared protocol and workflow records
- Validated package, tool, consumer, and release identities
- Docker Compose appliance definition and guest client environment
- Authenticated controller client

`PackageInfrastructureClient` remains one public type and API. Its private
implementation is grouped by endpoint domain around one transport/authentication
owner.

**Key Exports**: `PackageInfrastructureClient`, `RegistryEndpoints`, workflow
records, appliance resources

#### vm-package-server
**Role**: Private registry data plane and worker-local read edge.

**Responsibilities**:
- Native npm, Cargo, Python, and tool artifact protocols
- Authenticated central publication endpoints
- Read-only worker caching and approved public fallback
- Immutable internal artifact storage and metadata
- Protocol-owned validation with separate server routing and setup

Protocol handlers, storage, resolver internals, and validation modules are
private. The crate root exposes only server startup, state/config contracts,
the resolver service, upstream clients, hashes, and security validation needed
by binaries and integration tests.

**Key Exports**: Package server implementation, HTTP handlers, registry operations

#### vm-package-work
**Role**: Deterministic workflow state and immutable Git-bundle controller.

**Responsibilities**:
- Checkout leases and state transitions
- Durable source bundles and transient internal processing trees
- Submission, integration, release, rollout, and recovery receipts
- Durable tool activation plans, leases, target results, and repair state

**Key Exports**: `run`, `WorkCredentials`, workflow error contract. The router,
store, source manager, and persistence records remain service-internal.

#### vm-package-jobs
**Role**: Credential-scoped package workflow workers.

**Responsibilities**:
- Persistent bundle-isolated integration review
- Persistent private release publication
- Persistent isolated consumer upgrades
- Ephemeral tool publication
- Separate release workflow, source, archive, artifact, isolated-build, and
  publication ownership

**Key Exports**: Review, release, and rollout binaries

#### vm-auth-proxy
**Role**: Authentication and authorization proxy for VM services.

**Responsibilities**:
- Authentication proxy for development services
- Session management and token validation
- Security middleware for VM-exposed services
- OAuth and API key management

**Key Exports**: Auth proxy server, middleware, session management

## Dependency Flow

```mermaid
graph TD
    %% Foundation layer
    A[vm-core] --> B[vm-messages]
    A --> F[vm-platform]

    %% Configuration layer
    C[vm-config] --> A
    C --> B

    %% Provider layer
    D[vm-provider] --> A
    D --> C
    E[vm-temp] --> A
    E --> D

    %% Domain layer
    Q[vm-packages]

    %% Service layer
    H[vm-package-server] --> A
    H --> Q
    W[vm-package-work] --> A
    W --> Q
    J[vm-package-jobs] --> Q
    I[vm-auth-proxy] --> A

    %% Application layer
    L[vm] --> A
    L --> B
    L --> C
    L --> D
    L --> E
    L --> F
    L --> I
    L --> Q

    %% Installer
    M[vm-installer] --> A
    M --> C
```

## Error Handling Philosophy

Goobits VM uses the shared `VmError` type from `vm-core` at cross-crate
boundaries. Libraries return errors without printing them. The `vm` executable
adds final context and renders each fatal error once.

### Core Principles

1. **Structured errors**: Preserve the operation and failure category.
2. **Single rendering point**: Do not print a failure before returning it.
3. **Stable streams**: Requested data uses stdout; diagnostics use stderr.
4. **Safe context**: Never include credentials or raw secret-bearing arguments.

The enum in `vm-core/src/error.rs` is the canonical error-category inventory.

### Error Flow

1. **Origin**: A crate captures the local operation and source failure.
2. **Ownership**: Public cross-crate boundaries convert failures to `VmError`.
3. **Propagation**: Higher layers add context without printing the failure.
4. **Handling**: The application layer (`vm`) provides final formatting and feedback.

### Implementation Guidelines

- **Use the owning result type**: Keep local error details inside their crate.
- **Convert at boundaries**: Map external errors to `VmError` before crossing crates.
- **Add context once**: Describe the failed operation without printing it.
- **Fail fast**: Validate inputs early and fail with clear error messages
- **Log safely**: Use tracing for debugging without recording secret-bearing arguments.

## Testing Strategy

The layered architecture enables comprehensive testing at multiple levels:

- **Unit Tests**: Each crate includes extensive unit tests for its core functionality
- **Integration Tests**: Cross-crate functionality is tested through integration test suites
- **End-to-End Tests**: Complete workflows are tested through the main CLI interface
- **Mock Providers**: Test providers enable testing without external dependencies

The root [Testing Guide](../docs/development/testing.md) owns supported commands
and provider-isolation rules.
