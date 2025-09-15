# vm-pkg - Unified Package Manager for VM Tool

`vm-pkg` consolidates the functionality of three shell scripts (`install-cargo-package.sh`, `install-npm-package.sh`, `install-pip-package.sh`) into a single, robust Rust binary.

## Features

- **Unified Interface**: Single command for all package managers
- **Link Detection**: Automatically detects and uses linked local packages
- **Smart Python Handling**: Automatically chooses between pip and pipx
- **Wrapper Generation**: Creates wrapper scripts for pipx environments
- **Type Safety**: Rust's type system prevents common shell script errors

## Usage

### Install a Package

```bash
# Install a cargo package
vm-pkg install --type cargo ripgrep

# Install an npm package
vm-pkg install --type npm typescript

# Install a Python package (automatically uses pip or pipx)
vm-pkg install --type pip black

# Force registry install (ignore linked packages)
vm-pkg install --type npm --force-registry my-package
```

### Check if a Package is Linked

```bash
vm-pkg check --type cargo my-crate
vm-pkg check --type npm my-module
vm-pkg check --type pip my-package
```

### List Linked Packages

```bash
# List all linked packages
vm-pkg list

# List only npm linked packages
vm-pkg list --type npm
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
vm-pkg install --type cargo ripgrep --user developer
vm-pkg install --type npm typescript --user developer
vm-pkg install --type pip black --user developer
```

## Architecture

```
vm-pkg/
├── src/
│   ├── main.rs           # Entry point
│   ├── cli.rs            # Command-line interface
│   ├── package_manager.rs # Package manager abstraction
│   ├── link_detector.rs  # Linked package detection
│   └── installer.rs      # Installation logic
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
vm-pkg check --type npm my-module
# Output: 🔗 Package 'my-module' is linked for npm

# Test installation
vm-pkg install --type pip black
# Output: 📦 Installing pip package from registry: black
#         ✅ Installed black as CLI tool with pipx
```