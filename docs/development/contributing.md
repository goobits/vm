# 🤝 Contributing Guide

Welcome! This guide will help you contribute to the VM development environment project.

## 🎯 Quick Start for Contributors

### Development Setup
```bash
# 1. Fork and clone the repository
git clone https://github.com/your-username/vm.git
cd vm

# 2. Set up development environment
./install.sh  # Install the tool locally

# 3. Run tests to ensure everything works
cd rust && cargo test

# 4. Make your changes
# 5. Test your changes
# 6. Submit a pull request
```

## 🛠️ Development Workflow

### Testing Your Changes
```bash
# Run Rust tests
cd rust && cargo test

# Run with verbose output
cd rust && cargo test -- --nocapture

# Test specific modules
cd rust && cargo test vm_config
cd rust && cargo test vm_provider
```

### Code Style
```bash
# Check Rust code formatting
cd rust && cargo fmt --check

# Run Rust linting
cd rust && cargo clippy

# YAML validation (if yamllint is available)
yamllint configs/*.yaml examples/**/*.yaml

# Follow existing patterns in the codebase
# - Use consistent indentation (2 spaces for YAML, 4 for Rust)
# - Add comments for complex logic
# - Use descriptive variable names
# - Follow Rust naming conventions
```

### User-Facing Messages

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

## 📁 Project Structure

```
.
├── install.sh                      # Installation script
├── CLAUDE.md                       # Development notes
├── README.md                       # User documentation
├── configs/                        # Configuration files
│   └── defaults.yaml               # Default configuration
├── docs/                           # Documentation
│   ├── getting-started/            # Installation & quick start
│   ├── user-guide/                 # User documentation
│   └── development/                # Contributing guides
└── rust/                           # Rust workspace (main codebase)
    ├── Cargo.toml                  # Workspace configuration
    ├── vm/                         # Main CLI application
    ├── vm-core/                    # Foundation utilities & error handling
    ├── vm-config/                  # Configuration handling
    ├── vm-messages/                # Centralized user-facing messages
    ├── vm-package-manager/         # Package management
    ├── vm-platform/                # Platform detection
    ├── vm-provider/                # Provider implementations
    ├── vm-temp/                    # Temporary VM functionality
    ├── vm-installer/               # Installation management
    └── version-sync/               # Version synchronization
```

## 🎨 Types of Contributions

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

## 🎯 Framework Presets

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

#### Option 2: Add Detection Logic (For Core Framework Support)

```bash
# 1. Add detection logic in Rust
# Edit rust/vm-config/src/detector/
# Add detection patterns for your framework

# 2. Add tests
# Edit rust/vm-config/src/detector/tests/
# Add test for your framework detection

# 3. Create plugin for the preset
# Follow Option 1 to create the actual preset

# 4. Update documentation
# Edit docs/user-guide/presets.md
```

### Testing New Presets
```bash
# Test detection
echo "your-framework-project/" > /tmp/test
cd /tmp/test
touch your-framework.config.js
vm config preset your-framework  # Apply preset

# Test configuration
vm validate
```

## 🏗️ Provider Development

### Current Provider Architecture
Providers are implemented in Rust in the `rust/vm-provider/` package.

### Adding New Providers
```bash
# 1. Add provider implementation in Rust
# Edit rust/vm-provider/src/providers/
# Create your_provider.rs

# 2. Implement the Provider trait
# See rust/vm-provider/src/providers/docker.rs for example

# 3. Register provider in mod.rs
# Add your provider to the provider registry

# 4. Add tests
# Edit rust/vm-provider/src/tests/
```

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

## 🔬 Rust Architecture

### Current Rust Packages
The project is fully implemented in Rust with the following packages:

```bash
# Core packages
vm/              # Main CLI application
vm-core/         # Foundation utilities and error handling
vm-config/       # Configuration processing and validation
vm-messages/     # Centralized user-facing messages
vm-provider/     # Provider implementations (Docker, Vagrant, Tart)
vm-temp/         # Temporary VM functionality
vm-package-manager/          # Package management
vm-platform/     # Platform detection
vm-installer/    # Installation management
version-sync/    # Version synchronization

# Contributing to Rust components
cd rust/vm-config
cargo test
cargo build --release

# Integration testing
cp target/release/vm-config ../bin/
# Test with existing shell scripts
```

### Benchmarking
```bash
# Profile Rust binary performance
time ./vm create

# Profile Rust binary components (if individual binaries exist)
time ./rust/target/release/vm-config process vm.yaml
time ./rust/target/release/vm create --dry-run

# Compare before/after performance
hyperfine 'old_command' 'new_command'  # If hyperfine is available
```

## 📋 Code Review Checklist

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

## 🚀 Release Process

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

## 🎓 Learning Resources

### Understanding the Codebase
```bash
# Start with main entry point
less install.sh  # Installation script

# Understand the Rust implementation
less rust/vm/src/main.rs
less rust/vm-config/src/lib.rs
less rust/vm-provider/src/lib.rs
less rust/vm-core/src/lib.rs

# Review workspace structure
less rust/Cargo.toml
```

### Rust Development Best Practices
- Use `Result<T, E>` for error handling
- Implement proper error types with `thiserror`
- Use `clap` for CLI argument parsing
- Follow Rust naming conventions
- Add comprehensive unit tests
- Use `cargo fmt` and `cargo clippy`

### YAML Configuration
- Use 2-space indentation
- Include comments for complex configurations
- Validate with schema when possible
- Provide sensible defaults

## 🆘 Getting Help

### Discussion Channels
- GitHub Issues: Bug reports and feature requests
- GitHub Discussions: General questions and ideas
- Code Review: PR comments and suggestions

### Development Questions
When asking for help, include:
1. What you're trying to accomplish
2. What you've tried so far
3. Specific error messages
4. Relevant code snippets
5. Your development environment details

## 🎉 Recognition

Contributors are recognized in:
- CHANGELOG.md for significant contributions
- GitHub contributor graphs
- Release notes for major features

Thank you for contributing to making development environments better for everyone!