# Development Guide

This repository is a Rust workspace for the v5 humane CLI.

## Checks

The [Testing Guide](testing.md) owns supported checks, test layers, and
provider-isolation rules.

## CLI Architecture

Parsing lives in `rust/vm/src/cli`. Command dispatch lives in `rust/vm/src/commands`. Provider-specific behavior stays in `vm-provider`.

Shared terminal output lives in `vm-core/src/output_macros.rs`:

- Requested data and remote command output go to stdout.
- Progress, warnings, hints, and errors go to stderr.
- Commands return errors with context and an optional hint; `main.rs` renders them once.
- Provider and command modules must not print the same failure before returning it.

`vm --help` is the installed-version source of truth. The
[CLI Reference](../user-guide/cli-reference.md) owns the documented public
built-in command inventory. Do not maintain a second command list in
contributor docs.

## Lifecycle Hooks

Environment lifecycle commands register and unregister services through the
service registry. `vm shell`/`vm ssh` and `vm exec -- <command>` select the
project default when no name is supplied and start it if needed.
