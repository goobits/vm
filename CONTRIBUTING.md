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

# Enable git hooks (one-time setup)
git config core.hooksPath .githooks

# Install dependencies
cd rust
cargo build --workspace

# Install cargo-deny (one-time setup)
cargo install cargo-deny
```

The git hooks run `cargo fmt` and `cargo clippy` on commit to catch issues early.

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
3. **Dependency Audit** (`make audit`)
4. **Tests** (`make test`)

### Individual Quality Checks

You can also run each check individually:

```bash
# Check code formatting
make fmt

# Run clippy linter (warnings treated as errors)
make clippy

Our CI enforces a strict set of lints defined in `rust/clippy.toml`.
The build will fail if any of these lints are violated. Please run `make clippy`
locally to check your changes before submitting a pull request.

The current deny list includes:
- `clippy::uninlined_format_args`
- `clippy::redundant_clone`
- `clippy::unnecessary_wraps`

# Check dependencies for security advisories, license compliance, and bans
make audit

# Run all tests
make test
```

### Additional Quality Checks

In addition to the checks run by `make quality-gates`, we use several other tools to ensure code quality:

- **Code Duplication**: `jscpd`
- **Security Auditing**: `cargo-audit`
- **Code Complexity**: `rust-code-analysis-cli`
- **Test Coverage**: `cargo-tarpaulin`

For instructions on how to install and run these tools, please see the [Development Tools](docs/development/guide.md#development-tools) section in our developer guide.

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
│   ├── vm-messages/   # Centralized user-facing messages
│   ├── vm-logging/    # Logging setup and configuration
│   └── ...            # Other workspace crates (15 total)
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

## User-Facing Messages

All user-facing messages (e.g., status updates, errors, hints) MUST use the centralized `vm-messages` system to ensure consistency. Do not use `println!` or `eprintln!` directly for user output in application code.

The system is exposed through macros in the `vm-core` crate.

**How to Use:**

1. **Add a Template:** If a suitable message doesn't exist, add a new template to `rust/vm-messages/src/messages.rs`.
   ```rust
   // in MESSAGES struct
   pub my_new_message: &'static str,
   // in MESSAGES constant
   my_new_message: "✅ My new message for {item} is complete.",
   ```

2. **Use the Macros:** Use the `vm_println!`, `vm_error!`, or `vm_suggest!` macros in your code. Use the `msg!` macro for variable substitution.
   ```rust
   use vm_core::output_macros::{vm_println, msg};
   use vm_core::messages::MESSAGES;

   // Simple message
   vm_println!("{}", MESSAGES.some_static_message);

   // With variables
   vm_println!("{}", msg!(MESSAGES.my_new_message, item = "widgets"));
   // Output: ✅ My new message for widgets is complete.
   ```

## Types of Contributions

### 🐛 Bug Fixes
1. Check existing issues first
2. Create a failing test that reproduces the bug
3. Fix the issue
4. Ensure all tests pass
5. Submit PR with clear description

### ✨ New Features
1. Discuss the feature in an issue first
2. Write tests for the new functionality
3. Implement the feature
4. Update documentation
5. Ensure backward compatibility

### 📖 Documentation
1. Check for outdated information
2. Add examples and use cases
3. Improve clarity and organization
4. Test documentation steps manually

### 🧪 Testing
1. Add tests for untested functionality
2. Improve test coverage
3. Add edge case testing
4. Performance testing

## Framework Presets

### Adding New Framework Support

#### Option 1: Create a Plugin (Recommended)

```bash
# 1. Create plugin template
vm plugin new your-framework --type preset

# 2. Edit plugin metadata
# ~/.vm/plugins/presets/your-framework/plugin.yaml
name: your-framework
version: 1.0.0
description: Development environment for Your Framework
author: Your Name
plugin_type: preset

# 3. Configure preset content
# ~/.vm/plugins/presets/your-framework/preset.yaml
npm_packages:
  - your-framework-cli

services:
  - postgresql

environment:
  FRAMEWORK_ENV: development

# 4. Test the plugin
vm plugin validate your-framework
vm config preset your-framework
vm create

# 5. Share your plugin
# Package and share via git repository or tarball
```

## Provider Development

### Current Provider Architecture
Providers are implemented in Rust in the `rust/vm-provider/` package.

### Provider Trait
All providers must implement the Provider trait defined in Rust:
```rust
// Lifecycle methods
fn is_available(&self) -> Result<bool>;
fn create(&self, config: &Config) -> Result<()>;
fn start(&self, name: &str) -> Result<()>;
fn stop(&self, name: &str) -> Result<()>;
fn destroy(&self, name: &str) -> Result<()>;

// Access methods
fn ssh(&self, name: &str, path: Option<&str>) -> Result<()>;
fn exec(&self, name: &str, command: &[String]) -> Result<()>;

// Info methods
fn status(&self, name: &str) -> Result<ProviderStatus>;
fn logs(&self, name: &str) -> Result<String>;
fn list(&self) -> Result<Vec<String>>;
```

## Code Review Checklist

### Before Submitting PR
- [ ] All tests pass locally
- [ ] New functionality has tests
- [ ] Documentation is updated
- [ ] No breaking changes (or clearly documented)
- [ ] Code follows existing patterns
- [ ] Shell scripts pass basic validation
- [ ] Configuration examples work

### For Reviewers
- [ ] Code is well-structured and readable
- [ ] Tests adequately cover new functionality
- [ ] Documentation is accurate and helpful
- [ ] Performance impact is acceptable
- [ ] Security implications considered
- [ ] Backward compatibility maintained

## Code Review Process

1.  **Submission**: Once your pull request is submitted, a team member will be assigned to review it.
2.  **Automated Checks**: Our CI pipeline will automatically run all quality gates (`fmt`, `clippy`, `test`, etc.). Please ensure these pass.
3.  **First Review**: The reviewer will check for:
    -   Architectural and design soundness.
    -   Correctness and adherence to best practices.
    -   Code clarity and maintainability.
    -   Adequate test coverage.
    -   Documentation updates.
4.  **Feedback**: The reviewer may leave comments or request changes. Please address the feedback and push new commits to your branch. The pull request will update automatically.
5.  **Approval**: Once all feedback has been addressed and the reviewer is satisfied, they will approve the pull request.
6.  **Merge**: A maintainer will merge the pull request into the `main` branch.

We aim to provide an initial review within 2-3 business days. Thank you for your patience and contributions!

## Release Process

### Version Numbering
- **Major (x.0.0)**: Breaking changes
- **Minor (1.x.0)**: New features, backward compatible
- **Patch (1.1.x)**: Bug fixes, backward compatible

### Release Steps
1. Update CHANGELOG.md with new version
2. Test release candidate thoroughly
3. Update version in package.json (if applicable)
4. Tag release: `git tag v1.x.x`
5. Update documentation with new features
6. Announce release

## Getting Help

- Check existing issues and pull requests
- Review the documentation in `/docs`
- Ask questions in pull request comments

## License

By contributing to this project, you agree that your contributions will be licensed under the project's MIT license.
