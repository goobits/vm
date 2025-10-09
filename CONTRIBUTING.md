# Contributing to VM Tool

Thank you for your interest in contributing to the VM Tool project! This document provides guidelines and best practices for contributors.

## Development Setup

### Prerequisites

- Rust toolchain (stable channel)
- Docker (for integration tests)
- `cargo-deny` for dependency auditing

### Installation

```bash
# Clone the repository
git clone <repository-url>
cd vm

# Install dependencies
cd rust
cargo build --workspace

# Install cargo-deny (one-time setup)
cargo install cargo-deny
```

## Quality Gates

Before submitting any pull request, please ensure all quality gates pass. These checks help maintain code quality and consistency across the project.

### Running Quality Gates

```bash
# Run all quality gates at once
make quality-gates
```

This will run the following checks in order:

1. **Code Formatting** (`make fmt`)
2. **Linting** (`make clippy`)
3. **Dependency Audit** (`make deny`)
4. **Tests** (`make test`)

### Individual Quality Checks

You can also run each check individually:

```bash
# Check code formatting
make fmt

# Fix code formatting
make fmt-fix

# Run clippy linter (warnings treated as errors)
make clippy

# Check dependencies for security advisories, license compliance, and bans
make deny

# Run all tests
make test
```

### Pre-Commit Checklist

Before committing your changes:

- [ ] Run `make quality-gates` and ensure all checks pass
- [ ] Add tests for new functionality
- [ ] Update documentation if needed
- [ ] Follow the code style (enforced by `cargo fmt`)
- [ ] Ensure no clippy warnings

### Code Style

- Use `cargo fmt` to format your code before committing
- Follow Rust naming conventions and idioms
- Add doc comments for public APIs
- Keep functions focused and reasonably sized (<300 LOC)

### Testing

- Write unit tests for new functionality
- Integration tests for cross-crate features go in `vm/tests/`
- Use `#[cfg(test)]` modules for unit tests within source files
- Ensure tests pass on your local machine before pushing

### Dependency Management

We use `cargo-deny` to enforce:

- **License compliance**: Only permissive licenses (MIT, Apache-2.0, BSD, etc.)
- **Security advisories**: No known vulnerabilities
- **Dependency bans**: Avoid problematic crates
- **Source verification**: Dependencies from trusted registries only

If you add a new dependency, ensure it passes `cargo deny check`.

## Project Structure

```
vm/
├── rust/              # Rust workspace root
│   ├── vm/            # Main CLI binary
│   ├── vm-config/     # Configuration management
│   ├── vm-provider/   # Provider abstraction (Docker, Tart, Vagrant)
│   ├── vm-core/       # Shared core utilities
│   ├── vm-cli/        # CLI helpers and formatting
│   └── ...            # Other workspace crates
├── Makefile           # Build and quality gate commands
└── CONTRIBUTING.md    # This file
```

## Commit Messages

- Use clear, descriptive commit messages
- Follow conventional commits format when possible:
  - `feat:` for new features
  - `fix:` for bug fixes
  - `refactor:` for code refactoring
  - `docs:` for documentation changes
  - `test:` for test additions/changes
  - `chore:` for maintenance tasks

Example:
```
feat(provider): add Tart provider support

Implements the Provider trait for Tart virtualization,
including create, start, stop, and SSH operations.
```

## Pull Request Process

1. Fork the repository and create a feature branch
2. Make your changes following the guidelines above
3. Run `make quality-gates` to ensure all checks pass
4. Commit your changes with clear messages
5. Push to your fork and create a pull request
6. Address any review feedback

## Getting Help

- Check existing issues and pull requests
- Review the documentation in `/docs`
- Ask questions in pull request comments

## License

By contributing to this project, you agree that your contributions will be licensed under the project's MIT license.
