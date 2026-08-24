# Testing

The root `Makefile` owns the supported test and quality commands. It uses
`cargo-nextest` when available and falls back to `cargo test`.

## Daily Commands

| Goal | Command |
| --- | --- |
| Unit tests | `make test-unit` |
| Non-network integration tests | `make test-integration` |
| Unit and integration tests | `make test` |
| Network-dependent tests | `make test-network` |
| Formatting check | `make fmt` |
| Compile check | `cd rust && cargo check -j 2 --workspace --all-features` |
| Clippy | `make clippy` |
| Full local gate | `make quality-gates` |
| Docker package workflow | `scripts/internal/test-package-workflow-docker.sh` |

The Docker workflow entrypoint sources its assertions and scenarios from
`scripts/internal/package-workflow-docker/`; static fixture files live under
that directory instead of being embedded in the runner.

`make quality-gates` also requires `cargo-deny`, nightly Rust with
`cargo-udeps`, and any provider dependencies used by integration tests.

## Test Layers

### Unit Tests

Unit tests cover pure behavior without contacting providers:

```bash
make test-unit
# Equivalent fallback:
cd rust && cargo test --workspace --lib -- --test-threads=10
```

### Integration Tests

Integration targets cover configuration, package, provider, and CLI boundaries:

```bash
make test-integration
```

Tests that create real containers or VMs are marked `#[ignore]`. Run an ignored
test explicitly, serially, and only in an isolated environment:

```bash
cd rust
cargo test -p goobits-vm --test vm_ops test_name -- --ignored --test-threads=1
```

Do not run provider-mutating tests against a development environment that
contains uncheckpointed work or unique writable-layer data.

The Docker package-workflow acceptance test uses an isolated temporary home,
local appliance images, an appliance-volume Git remote, and a purpose-built
producer, consumer, and stopped project environment. CI runs it on every
main-branch push and pull request. It proves that package-scoped `open` enters
the original host bind without creating checkout state, plus automatic
workspace registration, exactly-once publication, fleet activation, executable
adoption with backups, restart/resume, deferred activation on start, and stable
container and volume identities. CI restarts the package controller. On a
disposable host, set `VM_ACCEPTANCE_DOCKER_RESTART_COMMAND` to a command that
restarts Docker itself to run the full daemon-interruption gate. The script
removes only its unique acceptance resources.

The supported integration target enables the package server's
`standalone-binary` feature so its CLI fixtures compile and run with the rest of
the non-network suite. Keep that feature in both the nextest and `cargo test`
paths when changing the root `Makefile`.

When a host is under file-descriptor or VM pressure, use formatting plus
`cargo check -j 2` as the non-mutating gate. Do not substitute a Docker/Tart
smoke test until the host has been recreated and its source mounts verified.

### Network Tests

Network tests contact upstream package registries and may request Keychain
access on macOS:

```bash
make test-network
```

They are not part of the normal `make test` path.

## Targeted Checks

Run the narrowest owning package first:

```bash
cd rust
cargo test -p vm-config
cargo test -p vm-provider
cargo test -p vm-core
cargo test -p goobits-vm --bin vm
```

Compile all test targets without running them:

```bash
cd rust
cargo test --workspace --all-features --no-run
```

## Adding Tests

- Put unit tests beside the implementation in `#[cfg(test)]` modules.
- Put public API and cross-module tests in the owning crate's `tests/`
  directory.
- Use temporary directories and unique resource names.
- Mark tests that mutate Docker, Podman, or Tart as ignored with a clear reason.
- Test both successful behavior and failure cleanup.
- Never weaken assertions or suppress warnings to make a check pass.

## Troubleshooting

Show test output:

```bash
cargo test test_name -- --nocapture
```

List matching tests:

```bash
cargo test -- --list
```

Run serially when debugging shared-resource behavior:

```bash
cargo test test_name -- --test-threads=1 --nocapture
```

Use `vm doctor` for provider diagnostics. Do not use broad Docker pruning as
test cleanup.
