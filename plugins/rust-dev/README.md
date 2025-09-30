# Rust Development Plugin

Systems programming environment with Cargo and Rust toolchain dependencies.

## What's Included

### System Packages
- `build-essential` - C/C++ compilers
- `libssl-dev` - OpenSSL development files
- `pkg-config` - Package configuration tool

### Optional Services
- Redis (enable manually if needed)
- PostgreSQL (enable manually if needed)

### Environment
- `RUST_BACKTRACE=1` - Enable backtraces
- `CARGO_TERM_COLOR=always` - Colored output

## Installation

```bash
vm plugin install plugins/rust-dev
```

## Usage

```bash
vm config preset rust
```

## License

MIT