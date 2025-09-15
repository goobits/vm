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
# Shell script formatting (if shellcheck is available)
# Check Rust code formatting
cd rust && cargo fmt --check

# Run Rust linting
cd rust && cargo clippy

# YAML validation (if yamllint is available)
yamllint configs/*.yaml test/configs/*.yaml

# Follow existing patterns in the codebase
# - Use consistent indentation (2 spaces for YAML, 4 for shell)
# - Add comments for complex logic
# - Use descriptive variable names
```

## 📁 Project Structure

```
.
├── vm                              # Main entry point (wrapper for Rust binary)
├── shared/                         # Shared utilities
│   ├── config-processor.sh         # Configuration handling
│   ├── project-detector.sh         # Framework detection
│   ├── provider-interface.sh       # Provider abstraction
│   └── temporary-vm-utils.sh       # Temp VM functionality
├── providers/                      # Provider implementations
│   ├── docker/                     # Docker provider
│   ├── vagrant/                    # Vagrant provider
│   └── tart/                       # Tart provider (Apple Silicon)
├── configs/                        # Configuration files
│   ├── presets/                    # Framework presets
│   └── schemas/                    # Validation schemas
├── docs/                           # Documentation
├── test/                           # Test suite
└── rust/                           # Performance improvements (WIP)
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
```bash
# 1. Create preset configuration
# configs/presets/your-framework.yaml
preset:
  name: "Your Framework"
  description: "Development environment for Your Framework"

# Add framework-specific configuration
npm_packages:
  - your-framework-cli

ports:
  dev: 3000

services:
  postgresql:
    enabled: true

# 2. Add detection logic
# Edit shared/project-detector.sh
detect_your_framework() {
    [[ -f "your-framework.config.js" ]] && echo "your-framework"
}

# 3. Add tests
# Edit test/unit/preset-detection.test.sh
test_detect_your_framework() {
    create_temp_project
    touch your-framework.config.js
    result=$(detect_project_type)
    assert_contains "$result" "your-framework"
}

# 4. Update documentation
# Edit docs/user-guide/presets.md
```

### Testing New Presets
```bash
# Test detection
echo "your-framework-project/" > /tmp/test
cd /tmp/test
touch your-framework.config.js
vm --preset your-framework create

# Test configuration
vm validate
vm preset show your-framework
```

## 🏗️ Provider Development

### Adding New Providers
```bash
# 1. Create provider directory
mkdir providers/your-provider

# 2. Implement provider interface (see providers/docker/provider.sh)
# Required functions:
# - provider_available()
# - provider_create()
# - provider_start()
# - provider_stop()
# - provider_destroy()
# - provider_ssh()
# - provider_status()

# 3. Add provider to detection logic
# Edit shared/provider-interface.sh

# 4. Add tests
# Edit test/system/vm-lifecycle.test.sh
```

### Provider Interface
All providers must implement these functions:
```bash
# Lifecycle
provider_available()     # Check if provider is available
provider_create()        # Create new VM/container
provider_start()         # Start stopped VM/container
provider_stop()          # Stop running VM/container
provider_destroy()       # Delete VM/container completely

# Access
provider_ssh()           # SSH into VM/container
provider_exec()          # Execute command in VM/container

# Info
provider_status()        # Get VM/container status
provider_logs()          # Get VM/container logs
provider_list()          # List all VMs/containers
```

## 🔬 Performance Improvements

### Shell to Rust Migration (WIP)
The project is gradually migrating performance-critical components to Rust:

```bash
# Current Rust projects
ls rust/
vm-config/    # YAML processing (replaces config-processor.sh)
vm-links/     # Package link detection (replaces link-detector.sh)
vm-ports/     # Port management (replaces port-manager.sh)

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
less vm  # Wrapper script

# Understand the Rust implementation
less rust/src/main.rs
less rust/vm-config/src/lib.rs
less rust/vm-provider/src/lib.rs

# Legacy shell scripts (being migrated)
less shared/config-processor.sh
less providers/docker/provider.sh
```

### Shell Scripting Best Practices
- Use `set -euo pipefail` for error handling
- Quote variables: `"$variable"` not `$variable`
- Use `[[` instead of `[` for conditionals
- Use functions for reusable code
- Add error checking for external commands

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