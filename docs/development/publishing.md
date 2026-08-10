# Publishing

Releases are distributed as GitHub Release binaries and installed with
`install.sh`. The CLI depends on private workspace crates, so crates.io
publication is intentionally disabled.

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
cargo build --manifest-path rust/Cargo.toml --package goobits-vm --release
```

Both commands must pass before tagging a release.

## Publish

After the changelog, version, and release commit are approved, create and push
the matching `vX.Y.Z` tag. The release workflow builds each supported target,
publishes archives and checksums, and creates the GitHub Release.

Publish the matching Tart Linux base from the `Publish Tart Linux base` manual
workflow. It requires an Apple Silicon self-hosted runner labeled
`tart-builder`; standard GitHub-hosted macOS runners do not support nested
virtualization. The workflow pushes both `vX.Y.Z` and `latest` to
`ghcr.io/goobits/vm-tart-linux`. Keep that GHCR package public so first-run
bootstrap does not require registry credentials.

Do not publish the internal workspace crates independently.

## Release Verification

Verify the tag points to the approved commit, every release archive has a
checksum, and the installer installs a `vm` binary with the same version.
