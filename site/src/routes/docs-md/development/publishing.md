# Publishing to crates.io

## Current Status

The `vm` CLI is configured for publishing to crates.io with:
- ✅ `publish = true` in `rust/vm/Cargo.toml`
- ✅ Metadata (description, repository, keywords, categories)
- ✅ MIT license
- ❌ Not yet published (awaiting maintainer action)

## Why `cargo install vm` Doesn't Work Yet

The workspace has `publish = false` at the top level (`rust/Cargo.toml:27`), which prevents accidental publishing of internal crates. The `vm` CLI overrides this with `publish = true`.

However, the package has not been published to crates.io yet, so users currently cannot run:
```bash
cargo install vm  # ❌ Error: could not find `vm`
```

## To Publish

### Prerequisites

1. **crates.io account**: Create at https://crates.io
2. **API token**: Get from https://crates.io/me
3. **Login**:
   ```bash
   cargo login <your-token>
   ```

### Publishing Steps

1. **Verify package name availability**:
   ```bash
   # Check if 'vm' is available on crates.io
   cargo search vm --limit 1
   ```

2. **Dry run** (recommended first):
   ```bash
   cd rust
   cargo publish --package vm --dry-run
   ```

3. **Publish**:
   ```bash
   cd rust
   cargo publish --package vm
   ```

### After Publishing

Update the README.md to reflect that `cargo install vm` now works:
```bash
# Install from Cargo (recommended)
cargo install vm  # ✅ Now works!
```

## Alternative: Use Different Package Name

If `vm` is taken on crates.io, consider alternatives:
- `vm-tool`
- `vm-dev`
- `devvm`
- `project-vm`

Update in `rust/vm/Cargo.toml`:
```toml
name = "vm-tool"  # or chosen alternative
```

## Notes

- Publishing is a one-way operation (you can yank versions but can't delete them)
- Version numbers must increment (can't republish the same version)
- The workspace configuration means only the `vm` binary will be published, not the internal library crates
- Users will still get the full functionality via `cargo install`, they just won't see the internal crates
