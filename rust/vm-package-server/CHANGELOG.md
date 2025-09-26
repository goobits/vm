# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2025-09-26

### Added
- Authentication support with Bearer token for package uploads
- Generic Registry API for cross-registry operations (count, list, delete)

### Changed
- Updated CHANGELOG to follow Keep a Changelog format
- Improved documentation organization

## [0.1.1] - 2025-09-23

### Added
- Comprehensive user configuration system with automatic package manager setup
- Strong typing for package identifiers and registry types
- Centralized file validation utilities for security
- Shared registry pattern helper for PyPI/NPM deduplication
- CLI module extraction for better code organization

### Changed
- Refactored architecture to eliminate 600+ lines of code duplication
- Extracted package validation logic into dedicated modules
- Improved stop command functionality and reliability
- Updated dependency versions with major version migrations
- Replaced config file approach with robust shell functions

### Fixed
- Resolved dead code warnings and updated Axum route syntax
- WebSocket message handling and Host header extraction
- Cargo sparse index support for better compatibility
- Installation script improvements and linting warnings

### Removed
- Unnecessary Git protocol support
- Legacy build artifacts and unused Docker deployment code
- Redundant validation code

## [0.1.0] - 2025-09-21

### Added
- Ultra-simple Docker deployment with `start --docker` command
- Local storage fallback for add, remove, and list commands
- Unified default port (3080) for both local and Docker modes
- Automatic package manager configuration on server start
- Client setup script available at `/setup.sh` endpoint

### Changed
- Simplified Docker deployment to single command
- Improved installation script with better error handling
- Enhanced E2E tests for background mode

### Fixed
- Cargo crate name and version extraction
- Background process handling in CLI

## [0.0.1] - 2025-09-18

### Added
- Initial release with full multi-protocol support for PyPI, npm, and Cargo
- Complete upload/publish functionality for all three registries
- File-based storage system with no external dependencies
- Docker support with multi-stage build for minimal image size
- Security hardening with path traversal protection
- Standardized API responses for consistency
- Comprehensive test suite
- GitHub Actions CI/CD pipeline

### Technical Foundation
- Built with Rust and Axum web framework
- Structured error handling with custom error types
- Centralized storage utilities for consistent file operations
- Performance optimized with hash pre-calculation for PyPI packages
- Non-root Docker user for enhanced security
- Configurable host, port, and data directory via CLI arguments