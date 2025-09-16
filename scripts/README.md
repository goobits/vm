# Project Scripts

This directory contains maintenance and development scripts for the VM project.

## Version Management

Version synchronization is now handled by the Rust-based `version-sync` tool in the workspace.

**Usage:**
```bash
# Check if versions are in sync
npm run check-versions

# Sync all versions to match package.json
npm run sync-versions
```

**Direct Rust usage:**
```bash
cd rust
cargo run --bin version-sync check
cargo run --bin version-sync sync
```

**What it syncs:**
- `rust/Cargo.toml` - Rust workspace version
- `defaults.yaml` - Default configuration template
- `rust/vm-config/config.yaml` - Embedded config template
- `rust/vm-config/vm.yaml` - VM config template

**Automatic sync:**
The version sync runs automatically when you use `npm version` to bump the version:

```bash
# This will bump version and sync all files
npm version patch
npm version minor
npm version major
```

### Design Philosophy

- **Single Source of Truth**: `package.json` version is the authoritative version
- **Rust-native**: Uses existing Rust toolchain, no additional dependencies
- **Automatic Sync**: Version changes trigger automatic synchronization
- **Validation**: CI can check that versions stay in sync
- **Schema vs Project Versions**: Configuration schema versions (like `"1.0"` in vm.yaml) are separate from project versions

### Adding New Version References

If you add new files that need version synchronization:

1. Add the file path to the `files_to_sync()` method in `rust/version-sync/src/main.rs`
2. Ensure the file uses the pattern `version: "x.x.x"` or `version = "x.x.x"`
3. Test with `npm run check-versions`

### Notes

- Schema versions (configuration format versions) should NOT be synced with project versions
- Test fixture versions in Rust code should remain as test data
- Dependency versions in Cargo.lock are managed by Cargo, not this script
- The version-sync tool is a full Rust crate in the workspace, ensuring type safety and consistency