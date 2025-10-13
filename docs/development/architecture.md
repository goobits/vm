# VM Tool Architecture Overview

For detailed Rust crate architecture, see [rust/ARCHITECTURE.md](../../rust/ARCHITECTURE.md).

This document provides a high-level overview of the entire project structure.

## Project Structure

```
vm/
├── configs/           # Embedded configuration templates
├── docs/             # User and developer documentation
├── examples/         # User-facing configuration examples
└── rust/             # Rust workspace with all crates
```

## Component Overview

### Configuration Management

**Embedded Configs** (`configs/`)
- Compiled into binaries at build time
- Production-grade templates with all options
- Used by `vm config preset` command

**User Examples** (`examples/`)
- Simplified examples for documentation
- Not embedded, purely illustrative
- Starting points for user projects

### Rust Workspace Packages

See [rust/ARCHITECTURE.md](../../rust/ARCHITECTURE.md) for comprehensive crate documentation.

**Quick Reference**:

| Package | Layer | Purpose |
|---------|-------|---------|
| `vm` | Application | Main CLI binary |
| `vm-core` | Foundation | Shared utilities, error handling |
| `vm-messages` | Foundation | User-facing message templates |
| `vm-cli` | Application | Message template variable substitution via `msg!` macro and `MessageBuilder` |
| `vm-config` | Configuration | Config parsing, validation, detection |
| `vm-provider` | Provider | VM provider abstraction (Docker/Vagrant/Tart) |
| `vm-temp` | Provider | Temporary VM management |
| `vm-platform` | Utility | Cross-platform system abstractions |
| `vm-package-manager` | Utility | Package manager integration |
| `vm-package-server` | Service | Private package registry |
| `vm-auth-proxy` | Service | Authentication proxy |
| `vm-docker-registry` | Service | Local Docker registry |
| `vm-installer` | Utility | Installation logic |
| `version-sync` | Meta | Cross-workspace version sync tool |

### Cross-Platform Build System

The project supports multiple target platforms:

**Supported Targets**:
- `x86_64-unknown-linux-gnu` (Linux x86_64)
- `aarch64-unknown-linux-gnu` (Linux ARM64)
- `x86_64-apple-darwin` (macOS Intel)
- `aarch64-apple-darwin` (macOS Apple Silicon)
- `x86_64-pc-windows-msvc` (Windows x86_64)

**Build Directories**:
- `rust/target/` - Default cargo build output
- `rust/target-linux-aarch64/` - Cross-compilation for Linux ARM64
- `rust/target-macos-aarch64/` - Cross-compilation for macOS ARM64

These target directories are managed by CI/CD workflows and are excluded from version control.

## Development Workflow

### Building from Source

```bash
cd vm/rust
cargo build --release
```

### Cross-Compilation

See `.github/workflows/release.yml` for cross-compilation setup:

```bash
# Example: Build for Linux ARM64
cargo build --workspace --release --target aarch64-unknown-linux-gnu
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run specific package tests
cargo test --package vm-config
```

See [docs/DEVELOPMENT.md](../DEVELOPMENT.md) and [docs/TESTING.md](../TESTING.md) for detailed testing instructions.

## Documentation Structure

```
docs/
├── getting-started/    # Installation and quick start
├── user-guide/         # Configuration, CLI reference, presets
└── development/        # Contributing, testing, architecture
```

## Related Documentation

- [docs/DEVELOPMENT.md](../DEVELOPMENT.md) - Development notes and testing
- [docs/TESTING.md](../TESTING.md) - Comprehensive testing documentation
- [rust/ARCHITECTURE.md](../../rust/ARCHITECTURE.md) - Detailed Rust crate architecture
- [docs/development/contributing.md](contributing.md) - Contribution guidelines