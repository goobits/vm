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
| Clippy | `make clippy` |
| Full local gate | `make quality-gates` |

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
