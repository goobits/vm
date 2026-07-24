# VM Rust Workspace

This workspace contains the Rust implementation of `vm`.

## Build

```bash
cargo build --workspace
```

## Test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## CLI Smoke

```bash
cargo run -p goobits-vm -- --help
cargo run -p goobits-vm -- run linux as dev --dry-run
cargo run -p goobits-vm -- list --dry-run
cargo run -p goobits-vm -- system update --dry-run
```

The public v5 command surface is intent-first: `run`, `list`, `shell`, `exec`,
`logs`, `copy`, `stop`, `restart`, `remove`, `save`, `revert`, `package`,
`config`, `tunnel`, `doctor`, `plugin`, and `system`.
