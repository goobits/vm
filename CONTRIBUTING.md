# Contributing to VM Tool

Thanks for contributing. This page owns contributor setup and review policy;
technical inventories stay in their canonical guides.

## Setup

Prerequisites:

- Stable Rust toolchain
- Docker for provider and package-workflow acceptance tests
- `cargo-deny`, nightly Rust, and `cargo-udeps` for the full quality gate

```bash
git clone <repository-url>
cd vm
git config core.hooksPath .githooks
make build-no-bump
```

Use `make build-no-bump` for ordinary development. `make build` changes the
project version before compiling.

## Workflow

1. Create a focused branch and make the smallest coherent change.
2. Add or update tests in the owning crate.
3. Update the canonical documentation when public behavior changes.
4. Run the narrowest relevant checks, then `make quality-gates` before review.
5. Commit with the repository's conventional-commit format and open a pull
   request with the behavior and verification summarized.

The [Testing Guide](docs/development/testing.md) owns supported checks, test
placement, and provider-isolation rules. Do not run provider-mutating tests
against an environment containing unique or uncheckpointed data.

## Code and Documentation

- Keep Rust formatted and free of Clippy warnings.
- Put unit tests beside their implementation and cross-module behavior in the
  owning crate's `tests/` directory.
- Follow the [Development Guide](docs/development/guide.md) for CLI dispatch and
  user-facing output behavior.
- Follow [Architecture](docs/development/architecture.md) for crate and provider
  boundaries.
- Treat `rust/vm/src/cli/` and generated `vm --help` as the command source of
  truth; keep the [CLI Reference](docs/user-guide/cli-reference.md) aligned with
  public built-in changes.
- Use the owners listed in the [documentation index](docs/README.md) instead of
  creating parallel command, test, configuration, or workflow inventories.

## Commits and Review

The commit hook requires `type(scope): description` or `type: description`.
Common types are `feat`, `fix`, `docs`, `refactor`, `test`, `perf`, `ci`,
`build`, and `chore`.

Before requesting review, confirm:

- The behavior is covered by focused tests.
- Relevant checks pass.
- Public docs describe current behavior and omit internal-only APIs.
- Security, compatibility, and resource-lifecycle effects are called out.
- The change contains no unrelated edits.

Maintainers review and merge approved pull requests into `main`.

## Focused Guides

- [Testing](docs/development/testing.md)
- [Architecture](docs/development/architecture.md)
- [Plugins](docs/user-guide/plugins.md)
- [Publishing](docs/development/publishing.md)

Check existing issues and pull requests before starting overlapping work. By
contributing, you agree that your contribution is licensed under the project's
MIT license.
