# Rust Development Plugin

Systems programming environment with Cargo and the Rust toolchain.

## What's Included

### System Packages
- `build-essential` - Essential tools for compiling software, including C/C++ compilers.
- `libssl-dev` - Development libraries for OpenSSL, required by many Rust crates.
- `pkg-config` - A helper tool used when compiling applications and libraries.

### Environment Variables
- `RUST_BACKTRACE` - Set to `1` to enable full backtraces on panic.
- `CARGO_TERM_COLOR` - Set to `always` to ensure colored output from Cargo.

### Included Services
This preset can be configured to use services, but none are enabled by default.
- **Redis** - In-memory data store.
- **PostgreSQL** - Relational database.

To enable a service, see the [Configuration](#configuration) section below.

## Installation

This plugin is automatically installed with the VM tool. No additional installation required.

To verify availability:
```bash
vm config preset --list | grep rust
```

## Usage

Apply this preset to your project:
```bash
vm config preset rust
vm create
```

Or add to `vm.yaml`:
```yaml
preset: rust
```

## Configuration

### Customizing Services
```yaml
preset: rust
services:
  postgresql:
    enabled: true
    database: my_rust_app_db
```

### Additional Packages
```yaml
preset: rust
packages:
  cargo:
    - cargo-watch
    - cargo-audit
```

## Common Use Cases

1. **Building a Rust Project**
   ```bash
   vm exec "cargo build --release"
   ```

2. **Running Tests**
   ```bash
   vm exec "cargo test"
   ```

## Troubleshooting

### Issue: Compilation errors related to OpenSSL
**Solution**: This preset installs `libssl-dev`, which should resolve most OpenSSL-related compilation issues. If you still encounter problems, ensure your `Cargo.toml` specifies a compatible version of the `openssl` crate.

### Issue: `error: linker 'cc' not found`
**Solution**: The `build-essential` package provides the necessary C compiler. If you see this error, run `vm apply` to ensure all system packages are installed correctly.

## Related Documentation

- [Configuration Guide](../../docs/user-guide/configuration.md)
- [Presets Overview](../../docs/user-guide/presets.md)
- [CLI Reference](../../docs/user-guide/cli-reference.md)

## License

MIT