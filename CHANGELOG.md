# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**Note**: This project has been migrated from shell scripts to Rust. Historical entries below reference the previous shell-based implementation (`vm.sh`, shell test scripts, etc.). The current implementation uses a Rust binary with a shell wrapper.

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

## [1.3.0] - 2025-09-05

### Added
- **Simple OS Configuration**: New `os` field for simplified VM setup
  - `os: ubuntu` → Docker provider with 4GB RAM, optimized for development
  - `os: macos` → Tart provider with 8GB RAM, native Apple Silicon support
  - `os: debian` → Docker provider with 2GB RAM, lightweight setup
  - `os: alpine` → Docker provider with 1GB RAM, minimal footprint
- **Tart Provider Support**: Native macOS/Linux virtualization on Apple Silicon
  - Automatic Tart provider selection for `os: macos`
  - ARM64 native performance with Rosetta 2 x86 emulation
  - SSH access and folder sharing configured automatically
  - Presets for Tart Ubuntu, macOS, and Linux configurations
- **Provider Auto-detection**: Intelligent provider selection based on OS and platform
  - Apple Silicon Mac + `os: macos` → Tart provider
  - Any platform + `os: ubuntu` → Docker provider (or Vagrant if specified)
  - Backward compatible with explicit `provider:` field

### Changed
- **Documentation Focus**: Repositioned `provider` field as advanced/explicit control option
- **Configuration Examples**: All basic examples now use simple `os` field approach
- **README**: Simplified quick start with `os:` field, moved provider details to advanced section
- **Zero-Config Claims**: Softened to "minimal configuration" for more accuracy

### Fixed
- **Tart Provider**: Added `vm logs` support for both macOS and Linux VMs
  - macOS: Uses `log stream` or `/var/log/system.log`
  - Linux: Uses `journalctl` or `/var/log/syslog`

### Added
- **Container-Friendly Structured Logging**: Implemented production-ready logging system with `shared/logging-utils.sh`
  - Support for `LOG_LEVEL` environment variable (DEBUG, INFO, WARN, ERROR)
  - Automatic routing: WARN/ERROR to stderr, INFO/DEBUG to stdout for container compatibility
  - Consistent log format: `timestamp | level | message | key=value context`
  - Simple logging functions: `vm_error()`, `vm_warn()`, `vm_info()`, `vm_debug()`
  - Integration into core scripts (vm.sh, shared/temporary-vm-utils.sh) with structured error reporting

### Fixed
- **Port Conflict Resolution**: Fixed issue where VM tool's own configuration was incorrectly inherited by user projects, causing port 3150 conflicts
- **Config Scanning**: Updated configuration discovery to exclude VM tool's workspace directory from upward scanning
- **Error Messages**: Fixed port conflict error messages showing incorrect port numbers (was showing container ID fragments)
- **Hostname Validation**: Added proper error handling for missing hostname field in Docker provisioning

### Changed
- **JSON Configuration Deprecation**: JSON configuration files (`.json`) are no longer supported. The project now exclusively uses YAML format (`vm.yaml`) for configuration files.
- **Default Ports**: Removed hardcoded port 3150 from VM tool's default configuration to prevent conflicts with user projects

### Added
- **Modular Architecture**: Extracted temporary VM functionality into `shared/temporary-vm-utils.sh` module for better maintainability and debugging
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