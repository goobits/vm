# VM Rust Workspace

This workspace contains the Rust implementation of `vm`.

## Build

```bash
cargo build --workspace
```

## Test

The root [Testing Guide](../docs/development/testing.md) owns supported checks,
test layers, and provider-isolation rules.

## CLI Smoke

```bash
cargo run -p goobits-vm -- --help
cargo run -p goobits-vm -- run linux as dev --dry-run
cargo run -p goobits-vm -- list --dry-run
cargo run -p goobits-vm -- system update --dry-run
```

The public v5 command surface is intent-first. The
generated `vm --help` output owns its exact inventory; the
[CLI Reference](../docs/user-guide/cli-reference.md) owns durable workflows.
