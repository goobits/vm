# Publishing

The supported end-user installation path is `install.sh`. Crates.io
publication is a maintainer workflow for the `goobits-vm` package, which
installs the `vm` binary.

## Version Ownership

- Root `package.json` is the release version input.
- `rust/Cargo.toml` owns the Rust workspace version.
- `version-sync` verifies that they match.
- Root `CHANGELOG.md` is the only project changelog.

Check synchronization:

```bash
cd rust
cargo run -p version-sync -- check
```

## Preflight

From a clean release commit:

```bash
make quality-gates
cd rust
cargo publish --package goobits-vm --dry-run
```

The dry run must pass before tagging or publishing. Keep crates.io credentials
outside the repository and never place tokens in command history, scripts, or
documentation.

## Publish

After the changelog, version, and tag are approved:

```bash
make publish
```

The Make target publishes `goobits-vm`. Do not publish the internal workspace
crates.

## Release Verification

Verify the tag points to the approved commit, the published package reports the
expected version, and the installer still installs a `vm` binary with that same
version.
