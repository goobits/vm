# vm-pkg - Unified Package Manager for VM Tool

`vm-pkg` consolidates the functionality of three shell scripts (`install-cargo-package.sh`, `install-npm-package.sh`, `install-pip-package.sh`) into a single, robust Rust binary.

## Features

- **Unified Interface**: Single command for all package managers
- **Link Detection**: Automatically detects and uses linked local packages
- **Auto Python Handling**: Automatically chooses between pip and pipx
- **Wrapper Generation**: Creates wrapper scripts for pipx environments
- **Type Safety**: Rust's type system prevents common shell script errors

## Usage

### Install a Package

```bash
# Install a cargo package
vm-pkg install --package-type cargo ripgrep

# Install an npm package
vm-pkg install --package-type npm typescript

# Install a Python package (automatically uses pip or pipx)
vm-pkg install --package-type pip black

# Force registry install (ignore linked packages)
vm-pkg install --package-type npm --force-registry my-package

# Specify user (defaults to "developer")
vm-pkg install --package-type cargo ripgrep --user developer
```

### Check if a Package is Linked

```bash
vm-pkg check --package-type cargo my-crate
vm-pkg check --package-type npm my-module
vm-pkg check --package-type pip my-package
```

### List Linked Packages

```bash
# List all linked packages
vm-pkg list

# List only npm linked packages
vm-pkg list --package-type npm
```

### Detect Linked Packages

```bash
# Detect if packages are linked
vm-pkg links detect npm express react
vm-pkg links detect cargo ripgrep fd-find
```

### Generate Docker Mounts

```bash
# Generate Docker mount strings for linked packages
vm-pkg links mounts npm express react
# Output: -v /home/developer/.links/npm/express:/workspace/node_modules/express:ro
```

## How It Replaces Shell Scripts

### Before (3 separate scripts):

```bash
# install-cargo-package.sh
./shared/install-cargo-package.sh developer ripgrep

# install-npm-package.sh
./shared/install-npm-package.sh developer typescript

# install-pip-package.sh (147 lines of complex logic!)
./shared/install-pip-package.sh developer black
```

### After (1 unified binary):

```bash
vm-pkg install --package-type cargo ripgrep --user developer
vm-pkg install --package-type npm typescript --user developer
vm-pkg install --package-type pip black --user developer
```

## Architecture

```
vm-pkg/
├── src/
│   ├── main.rs           # Entry point
│   ├── cli.rs            # Command-line interface
│   ├── package_manager.rs # Package manager abstraction
│   ├── link_detector.rs  # Linked package detection
│   ├── installer.rs      # Installation logic
│   └── links/           # Package link management
│       ├── mod.rs
│       ├── cargo.rs      # Cargo-specific link detection
│       ├── npm.rs        # NPM-specific link detection
│       ├── pip.rs        # Pip-specific link detection
│       └── system.rs     # System-wide link detection
├── tests/
│   └── links_integration_tests.rs
└── Cargo.toml
```

## Advantages Over Shell Scripts

1. **Error Handling**: Proper Result types instead of `set -e`
2. **Type Safety**: Package manager types are enforced at compile time
3. **Performance**: Faster execution, especially for link detection
4. **Maintainability**: 500 lines of Rust vs 210 lines of shell across 3 files
5. **Testing**: Can use Rust's built-in test framework
6. **Cross-platform**: Easier to support different platforms

## Integration with vm binary

The main `vm` binary can now call:

```bash
vm-pkg install --type "$package_type" "$package" --user "$PROJECT_USER"
```

Instead of:

```bash
source "./shared/install-${package_type}-package.sh"
install_${package_type}_package "$PROJECT_USER" "$package"
```

## Complex Logic Simplified

The most complex script (`install-pip-package.sh` with 147 lines) handled:
- Pipx environment detection
- Wrapper script generation
- Editable vs registry installation
- Multiple Python project formats

This is now cleanly organized in Rust with proper error handling and clear control flow.

## Build

```bash
cd /workspace/rust
cargo build --release
# Binary will be at: target/release/vm-pkg
```

## Testing

```bash
# Test with a linked package
mkdir -p /home/developer/.links/npm/my-module
vm-pkg check --package-type npm my-module
# Output: 🔗 Package 'my-module' is linked for npm

# Test with non-linked package
vm-pkg check --package-type npm non-linked-module
# Output: 📦 Package 'non-linked-module' is not linked (would install from registry)

# Test installation
vm-pkg install --package-type pip black
# Output: 📦 Installing pip package from registry: black
#         ✅ Installed black as CLI tool with pipx

# Run integration tests
cargo test --package vm-pkg
```