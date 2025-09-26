# Contributing to goobits-pkg-server

Thank you for your interest in contributing to goobits-pkg-server! This document provides guidelines for contributing to the project.

## Development Setup

### Prerequisites
- Rust 1.70+ (latest stable recommended)
- Git

### Local Development
1. Clone the repository:
   ```bash
   git clone <repository-url>
   cd goobits-pkg-server
   ```

2. Install dependencies and build:
   ```bash
   cargo build
   ```

3. Run tests:
   ```bash
   cargo test
   ```

4. Run the server locally:
   ```bash
   cargo run
   ```

## Code Quality Standards

### Formatting
All code must be formatted with `rustfmt`:
```bash
cargo fmt
```

### Linting
All code must pass `clippy` with no warnings:
```bash
cargo clippy -- -D warnings
```

### Testing
- All new features must include comprehensive tests
- Tests must pass locally and in CI
- Run tests with: `cargo test --verbose`

### Documentation
- Public APIs must be documented
- Complex logic should include inline comments
- Update README.md for user-facing changes

## Contribution Workflow

1. **Fork** the repository
2. **Create** a feature branch from `main`
3. **Make** your changes following our code quality standards
4. **Test** your changes thoroughly
5. **Commit** with clear, descriptive messages
6. **Push** to your fork
7. **Submit** a pull request

### Commit Message Format
Use clear, concise commit messages:
```
feat: add upload validation for PyPI packages
fix: resolve race condition in index generation
docs: update API documentation for cargo endpoints
test: add integration tests for npm publish
```

### Pull Request Guidelines
- Fill out the pull request template completely
- Ensure all CI checks pass
- Include relevant tests for your changes
- Update documentation as needed
- Keep PRs focused and reasonably sized

## Code Organization

### Project Structure
```
src/
├── main.rs          # Application entry point and routing
├── lib.rs           # Library exports and module declarations
├── config.rs        # Configuration management
├── error.rs         # Error types and handling
├── state.rs         # Application state management
├── storage.rs       # File storage operations
├── validation.rs    # Input validation utilities
├── pypi.rs          # PyPI registry handlers
├── npm.rs           # npm registry handlers
├── cargo.rs         # Cargo registry handlers
├── ui.rs            # Web UI handlers and templates
├── api.rs           # Management API endpoints
├── setup.rs         # Client setup script generation
├── client_ops.rs    # Package manager configuration
├── deletion.rs      # Package deletion operations
└── package_utils.rs # Shared package utilities
```

### Error Handling
- Use the custom `AppError` type for all errors
- Provide specific, actionable error messages
- Log errors with appropriate context
- Use the `?` operator for error propagation

### Logging
- Use structured logging with `tracing`
- Include relevant context in log messages
- Use appropriate log levels (debug, info, warn, error)

### Testing
- Write unit tests for individual functions
- Include integration tests for API endpoints
- Use temporary directories for file system tests
- Mock external dependencies appropriately

## CI/CD Pipeline

The project uses GitHub Actions for continuous integration:

### Required Checks
- **Formatting**: `cargo fmt --check`
- **Linting**: `cargo clippy -- -D warnings`
- **Building**: `cargo build --verbose`
- **Testing**: `cargo test --verbose`
- **Security**: `cargo audit`
- **Documentation**: `cargo doc`

### Additional Checks
- Code coverage reporting
- Release build verification
- Integration test validation
- Server startup testing

## Security

### Reporting Security Issues
Please report security vulnerabilities privately to the maintainers.

### Security Best Practices
- Never commit secrets or credentials
- Validate all user inputs
- Use secure defaults
- Follow Rust security guidelines

## Performance

### Performance Considerations
- Use async/await for I/O operations
- Minimize allocations in hot paths
- Profile performance-critical code
- Consider memory usage for large deployments

### Benchmarking
Run benchmarks for performance-sensitive changes:
```bash
cargo bench
```

## Documentation

### Code Documentation
- Document all public APIs with `///` comments
- Include examples in documentation
- Document error conditions and edge cases

### User Documentation
- Update README.md for user-facing changes
- Include configuration examples
- Document deployment considerations

## Release Process

### Version Management
This project follows [Semantic Versioning](https://semver.org/):
- MAJOR: Breaking changes
- MINOR: New features (backward compatible)
- PATCH: Bug fixes (backward compatible)

### Release Checklist
1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Ensure all tests pass
4. Create release tag
5. Publish release notes

## Getting Help

### Communication
- GitHub Issues: Bug reports and feature requests
- GitHub Discussions: Questions and community discussion
- Pull Requests: Code contributions and reviews

### Resources
- [Rust Documentation](https://doc.rust-lang.org/)
- [Axum Documentation](https://docs.rs/axum/)
- [Tokio Documentation](https://docs.rs/tokio/)

Thank you for contributing to goobits-pkg-server! 🚀