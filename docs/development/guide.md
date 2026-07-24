# Development Guide

This repository is a Rust workspace for the v5 humane CLI.

## Checks

```bash
cd rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## CLI Architecture

Parsing lives in `rust/vm/src/cli`. Command dispatch lives in `rust/vm/src/commands`. Provider-specific behavior stays in `vm-provider`.

Shared terminal output lives in `vm-core/src/output_macros.rs`:

- Requested data and remote command output go to stdout.
- Progress, warnings, hints, and errors go to stderr.
- Commands return errors with context and an optional hint; `main.rs` renders them once.
- Provider and command modules must not print the same failure before returning it.

Core public commands:

```text
run, list, shell, exec, logs, copy, stop, restart, remove, save,
revert, package, config, tunnel, doctor, plugin, system
```

Plugin-backed top-level commands:

```text
db, fleet, secret
```

## Lifecycle Hooks

Environment lifecycle commands register and unregister services through the service registry. Shell access uses `vm shell <name>`, and one-off commands use `vm exec <name> -- <command>`.
