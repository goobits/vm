# Contributing to VM Tool

Thanks for contributing. This page owns contributor setup and review policy;
technical inventories stay in their canonical guides.

## Setup

Prerequisites:

- Stable Rust toolchain
- Docker for provider and package-workflow acceptance tests
- `cargo-deny`, nightly Rust, and `cargo-udeps` for the full quality gate

```bash
git clone https://github.com/goobits/vm.git
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

## Opt-in CI

All repository workflows run only when explicitly dispatched. Pushes, pull
requests, tags, and schedules do not start CI or publish releases automatically.
Use **Actions → select a workflow → Run workflow**, choosing the branch to test,
or run only the checks needed for the change:

```bash
gh workflow run ci.yml --ref <branch>
gh workflow run security.yml --ref <branch>
gh workflow run coverage.yml --ref <branch>
gh run list --branch <branch>
```

The CI dispatch includes live package acceptance by default. When it has already
passed for unchanged package code, skip that lengthy job with
`gh workflow run ci.yml --ref <branch> -F package_acceptance=false`.

Dispatch PR checks on the PR's source branch. README badges show the latest
completed main-branch run, which may predate the latest commit. Dependabot alerts,
secret scanning, and push protection remain enabled independently of CI.
Release publishing requires an explicit dispatch on a verified release tag; see
[Publishing](docs/development/publishing.md).

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

## Changelog

Update [CHANGELOG.md](CHANGELOG.md) in the same PR when users need to know about
a change. Treat it as release notes, not a commit log.

- Include new capabilities, breaking changes with migration guidance, meaningful
  fixes, and security changes that affect users. Omit entries with no user impact.
- Use one short bullet per outcome: what changed and why it matters. Name the
  affected command or behavior precisely; verify command syntax against help.
- Edit or combine an existing Unreleased entry when work completes the same
  outcome. Do not stack implementation steps or repeat highlights elsewhere.
- Keep a clean layout: `Breaking changes`, `Added`, `Changed`, `Fixed`, `Security`
  only as needed, in that order. Omit empty sections, decorative emoji, nested
  lists, hype, and vague claims such as “improved reliability.”
- Leave refactors, routine dependency bumps, CI changes, test counts, acceptance
  logs, branch cleanup, and internal bookkeeping in PRs or development docs.
  Mention a dependency only when its update fixes a relevant security or
  compatibility issue; explain the impact and link the advisory when useful.
- Add changes under `Unreleased`; do not invent a release date or version. Keep
  published release history intact. At release time, use the actual tag/date.

Example: “Snapshot import preserves the existing snapshot when replacement
verification fails.” Avoid: “Refactor snapshot staging and add regression tests.”

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
