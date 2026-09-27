# Dependency PR and security acceptance — 2026-09-27

Code revision: `90b93e2f2aa4d00e5cffc9217dca5db41989669e`, based on
`ef2dc5e1c1337c8d8bb532e802cb896fc835cb61`. Integration:
[PR #89](https://github.com/goobits/vm/pull/89).

## PR dispositions

All 18 pre-existing open PRs were reviewed and closed with an explanation; their
Dependabot branches were deleted. Compatible changes were consolidated in #89:

| PRs | Result |
| --- | --- |
| #53, #76, #77 | SHA-1 0.11, clap_complete 4.6.9, axum-test 21; narrow compatible lockfile changes |
| #66, #68, #71 | Rust 1.98 image builders with verified immutable digests |
| #72, #73, #75, #78, #79, #82, #83, #84, #85, #86 | Reviewed immutable GitHub Actions updates |
| #67 | Declined Ubuntu 26 FROM-only migration: Vibe still requires Jammy-specific installation; refreshed supported 22.04 and aligned CLI runtime with Bookworm builder |
| #80 | Declined Node 26 FROM-only migration: Current rather than LTS and removes bundled Corepack required by the Dockerfile; refreshed Node 22 LTS |

The ongoing [dependency maintenance policy](dependency-maintenance.md) records
support dates, upgrade prerequisites, runner requirements, and security checks.
Unrelated branches with unique work are preserved.

## Validation

Local host: macOS 27.0 (26A428), arm64, Rust/Cargo 1.98.1.

```sh
cargo test --manifest-path rust/Cargo.toml --workspace --all-features
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --all-features -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo build --manifest-path rust/Cargo.toml -p goobits-vm --release --all-features
bash scripts/internal/check-cli-reference.sh
git diff --check
```

All passed: 1,201 tests, zero failures, 24 existing ignored tests. Workflow and
Dependabot YAML parsed successfully. No parser changes required regeneration.

Hosted validation passed at the code revision above:

- [CI run 36311622463](https://github.com/goobits/vm/actions/runs/36311622463):
  Linux/macOS/Windows tests, Clippy/formatting, dependency and duplication audit,
  release artifact upload/download byte comparison, live package acceptance.
- [Coverage run 36311622439](https://github.com/goobits/vm/actions/runs/36311622439).
- [Security run 36311622482](https://github.com/goobits/vm/actions/runs/36311622482):
  cargo-deny plus CodeQL security-extended analysis for Rust and GitHub Actions.

Live package acceptance used `scripts/internal/test-package-workflow-docker.sh`
with the built release binary and `VM_ACCEPTANCE_DOCKER_RESTART_COMMAND='sudo
systemctl restart docker'` on the disposable hosted runner. Runtime: Linux
6.17.0-1022-azure x86_64, Docker client/server 28.0.4, Compose 2.38.2,
Rust/Cargo 1.98.1. The workflow passed at 10:27:46 UTC. It covers background release,
receipt resume/interruption/retry, publication and consumer review, running
activation and stopped-target deferral, truthful partial failures, tools refresh
and disable, package service backup/restore/removal, and unchanged primary
container/package-volume identities.

The native arm64 `docker build -f Dockerfile.ci -t vm-security-ci:20260927 .`
and `docker run --rm vm-security-ci:20260927` passed, proving embedded assets,
compatible runtime libc, and the final default help command. The smoke-test image
was removed afterward. The first local package
acceptance attempt failed while downloading `pycparser` from files.pythonhosted.org
with a read timeout during Docker instability; it did not reach package operations.
Hosted acceptance independently passed with fresh images. The local retry then
passed end to end on Docker 29.8.0 / Compose 5.5.1, using the same release binary
and a test-only package-controller restart (the host daemon was not restarted by
the test). Its disposable containers and volumes were removed; the existing
`vm-dev` and `sketch-api-dev` guests remained running. The failed-attempt scratch
directory and task-only logs were removed after recording this evidence.

## Security results and limits

Repository settings now enable Dependabot alerts/security updates, secret
scanning, and secret push protection. Rust and Actions CodeQL scans run on PRs,
main pushes, weekly, and manually. After both initial scans completed, the
Dependabot, secret-scanning, and code-scanning APIs each reported zero open alerts;
the PR merge analysis commit was `b7c7fa7e98f7fcf216b4f1b905d6c6ec3436dc53`,
with zero results and no errors for both languages. No alert was dismissed to
obtain this result. Dependency policy passed without
advisory exceptions.

Release actions were reviewed and artifact transport was exercised. No production
release, tag, or image was published as a test. The protected self-hosted Tart
publisher and a full Ubuntu/Node major migration were not exercised by this
maintenance task. The earlier full provider acceptance remains recorded in the
CLI acceptance documents; these scans do not guarantee absence of vulnerabilities.
