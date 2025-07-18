# Global Development Preferences

*Use these preferred tools when relevant to the current project.*

## JavaScript Projects
- Use `pnpm` for package management
- Use `eslint` for linting and formatting (with --fix flag)
- Use JSDoc for type annotations

## Python Projects
- Use `pipx` to install Python CLI tools
- Use `ruff` for linting and formatting
- Use `pytest` for unit testing
- Use `mypy` for type checking

## Rust Projects
- Use `cargo` for package management
- Use `rustfmt` for formatting
- Use `clippy` for linting
- Use `cargo test` for testing

## Tech Stack
- Use PostgreSQL for relational databases
- Use Redis for caching and session storage
- Use Docker for containerization
- Use nginx for web server

## Dependencies
- Always check existing package.json/pyproject.toml/Cargo.toml before assuming what's available
- Use existing project dependencies when possible

## Environment
- Use environment variables for configuration/secrets
- Never commit API keys or sensitive data

## Testing
- Always run tests before making changes to understand current state
- Always run tests after making changes to ensure no regressions
- If tests fail after changes, fix them before proceeding

## Git & Commits
- Never commit changes unless the user specifically requests it
- Only commit when tests pass and code is working

## Temporary Files
When creating temporary debug or test scripts, use `/tmp` directory to keep the project clean.