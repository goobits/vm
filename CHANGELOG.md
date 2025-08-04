# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] - 2025-08-02

### Added
- **Smart Preset System**: A major new feature that provides a "zero-config" startup experience.
  - **Automatic Project Detection**: Scans project files to identify frameworks (React, Django, Rails, Vue, etc.) and tools (Docker, Kubernetes).
  - **Preset Application**: Automatically applies a layered configuration based on detected technologies.
  - **Multi-Technology Support**: Intelligently merges presets for complex projects (e.g., a React frontend with a Django backend).
  - **User Control Flags**:
    - `--no-preset`: Disables the preset system entirely for manual configuration.
    - `--preset <name>`: Forces a specific preset, overriding detection.
    - `--interactive`: Provides an interactive menu to review, add, or remove presets before VM creation.
  - **Preset Management Commands**:
    - `vm preset list`: Shows all available presets with descriptions.
    - `vm preset show <name>`: Displays the full configuration of a specific preset.
- **New Presets**: Added presets for `react`, `vue`, `next`, `angular`, `django`, `flask`, `rails`, `docker`, and `kubernetes`.
- **Comprehensive Test Suite**: Added `test/test-presets.sh` to validate all aspects of the new preset system.
- **Documentation**:
  - Created `PRESETS.md` with a detailed guide to the new system and customization examples.
  - Updated `README.md` to highlight the new feature.

### Changed
- The `load_config` function in `vm.sh` now delegates to a more powerful `load_config_with_presets` function in `shared/config-processor.sh`.
- The `detect_project_type` function in `shared/project-detector.sh` is now significantly more advanced, capable of identifying specific frameworks.

## [Unreleased]

### Changed
- **JSON Configuration Deprecation**: JSON configuration files (`.json`) are no longer supported for direct use. Users must migrate existing JSON configs to YAML format using `vm migrate` command. This simplifies the configuration system while maintaining backward compatibility through migration.

### Added
- **Modular Architecture**: Extracted temporary VM functionality into separate `vm-temporary.sh` module for better maintainability and debugging
- **Configuration Migration**: Full support for migrating legacy JSON configs to modern YAML format with versioning
  - `vm migrate --check` - Check if migration is needed
  - `vm migrate --dry-run` - Preview migration changes  
  - `vm migrate` - Perform the migration with automatic backup
- **Enhanced Temporary VMs**: Improved temp VM workflow with dedicated subcommands
  - `vm temp ./src,./config` - Create temp VM with specific directory mounts
  - `vm temp ssh` - Direct SSH into active temp VM
  - `vm temp status` - View temp VM configuration and state
  - `vm temp destroy` - Clean up temp VM and all resources
  - `vm tmp` - Short alias for `vm temp`
- **Test Suite**: Renamed `test-runner.sh` to `test.sh` for simpler convention
- **YAML Configuration**: Modern YAML-based configuration with comments, better readability, and schema validation

### Changed
- Reduced main `vm.sh` from 1,903 lines to 1,446 lines (24% reduction) through modular extraction
- Improved error handling and debug output in temp VM functionality
- Updated test documentation to reflect actual implementation

### Fixed
- Resolved hanging issues with `vm temp` command through modular architecture
- Fixed installation script path resolution to work from any directory
- Corrected test runner references throughout the codebase

### Removed
- Removed monolithic temp VM code from main script (now in separate module)
- Cleaned up backup files after successful module extraction

## [1.0.0] - Previous Release

### Added
- Initial VM infrastructure with Docker and Vagrant providers
- YAML configuration system with schema validation
- Temporary VM functionality
- Configuration migration tools
- Comprehensive test suite
- Terminal themes and customization options