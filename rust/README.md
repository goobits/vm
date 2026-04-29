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
cargo run -p goobits-vm -- ls --dry-run
cargo run -p goobits-vm -- system update --dry-run
```

The public v5 command surface is intent-first: `run`, `ls`, `shell`, `exec`, `logs`, `copy`, `stop`, `rm`, `save`, `revert`, `package`, `config`, `tunnel`, `doctor`, `plugin`, and `system`.
