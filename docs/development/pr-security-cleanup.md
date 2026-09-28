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
scanning, and secret push protection. Rust and Actions CodeQL scans initially ran on PRs,
main pushes, weekly, and manually; the later opt-in policy below supersedes those triggers. After both initial scans completed, the
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

## Follow-up after dependency scanning populated

After #89 merged, Dependabot populated additional PRs #90–101 and alert #1 for
[`tar` PAX header desynchronization](https://github.com/advisories/GHSA-3pv8-6f4r-ffg2).
The initial empty alert response above was a point-in-time result, not completion
of the asynchronous dependency scan. [PR #102](https://github.com/goobits/vm/pull/102)
updates the lockfile to tar 0.4.46 and requires that minimum in the workspace.

- #90 was declined: Ubuntu 22.10 has been unsupported since July 20, 2023.
  Ubuntu minor-version changes are distro migrations too; Dependabot now excludes
  those as well as major migrations while retaining digest updates.
- #91 updates the immutable Buildx action; #92 applies the archive security fix.
- #93–101 consolidate TOML 1.1.6, tokio-util 0.7.19, clap 4.6.7, reqwest 0.13.5,
  serial_test 4.0.1, futures-util 0.3.34, indexmap 2.14.2, flate2 1.1.10,
  and which 8.0.6. The test-only serial_test major update requires Rust 1.93.1,
  below the Rust 1.98.1 toolchain used here. No compatibility shim was added.
- Compatible Cargo updates and workflow actions are grouped to reduce recurring
  PR noise. Superseded individual CI runs were cancelled to free runner capacity.
- `CONTRIBUTING.md#changelog` defines the user-facing release-note policy, linked
  from `AGENTS.md`. Unreleased notes were condensed into user-facing outcomes;
  published 5.0.0 history is byte-for-byte unchanged. Routine dependency and CI
  details remain here, outside the changelog.

Resolved dependency revision: `1834c6f72a6c67536f969168bcf9c2c5e7446f26`.
The final tar minimum-version declaration selects the same checked lockfile.
Local workspace tests passed (1,201 passed, 24 existing ignored), as did Clippy,
formatting, generated CLI reference, locked Cargo metadata, the all-features
release build, and `git diff --check`.

Follow-up validation runs:

- [CI 36314787088](https://github.com/goobits/vm/actions/runs/36314787088).
- [Coverage 36314787070](https://github.com/goobits/vm/actions/runs/36314787070).
- [Security 36314787095](https://github.com/goobits/vm/actions/runs/36314787095).
  Both CodeQL languages reported zero findings and no analysis errors at PR merge
  revision `272b55fb31685fe6d6bf9b37d1e2dafa9ee79ada`. Earlier cancelled runs are
  superseded by these complete analyses.

## Main-branch CodeQL follow-up

The later main-branch analysis reported 33 alerts on `097be68b`. The zero-result
PR analyses above did not establish a clean main-branch security state.
[Per-alert review](code-scanning-triage.md) records the actual flows, test-only
fixtures, and false-positive evidence. PR #102 fixes PyPI HTML injection and auth
transport boundaries. The review also found and fixed scoped consumer receipt
persistence. No CodeQL rule category is disabled, and genuine findings remain
open until the integrated source is rescanned.

Security-fix revision: `63f954bedbdf2028eb21a6a380ae9e652aa0e4f3`.
Local checks passed: 1,210 tests, zero failures, 24 existing ignored tests;
Clippy (all targets/features, warnings denied), formatting, generated CLI reference,
all-features release build, and whitespace checks. No parser regeneration was needed.

Hosted checks passed at that revision:

- [CI 36324862930](https://github.com/goobits/vm/actions/runs/36324862930):
  Linux/macOS/Windows, Clippy/audit, artifact roundtrip, and live Docker package acceptance.
- [Coverage 36324862961](https://github.com/goobits/vm/actions/runs/36324862961).
- [Security 36324862940](https://github.com/goobits/vm/actions/runs/36324862940).
  Rust and Actions PR analyses completed without analysis errors. Main-branch alert
  state must still be verified after integration.

README badges link to live main-branch CI, security, and coverage workflow status.
They do not imply a numeric coverage percentage or absence of vulnerabilities.

## Dependency migration and opt-in checks — 2026-09-28

After #102, PRs #103–108 were merged into main (`1cf77e53`). The compatible
update group passed CI, but the additional AES-GCM, sysinfo, and Tera migrations
introduced compile/render failures. The repair uses sysinfo's static physical-core
query on every platform, OS randomness through rand's `SysRng`, Tera's explicit
optional access and indexed tuples, and a Compose-specific YAML scalar encoder.
Service-port collection now happens in Rust rather than template mutation.
No fallback dependency version or compatibility implementation was added.

Local workspace validation after the repair: 1,210 passed, zero failed,
24 existing ignored tests; all-target/all-feature Clippy passed. Existing tests
cover crypto roundtrips/persistence, all 214 provider cases, and CLI output.

At the owner's request, commit `52c779bd` makes all six repository workflows
manual-only, including security, coverage, onboarding, and release publishing.
No active branch-protection or repository rule requires automatic checks.
Dependabot alerts, secret scanning, and push protection remain enabled. Badge
captions and contributor/publication instructions explain explicit dispatch.
The onboarding workflow now reuses isolated provider acceptance instead of
retired `run`, `status`, and `shell --command` calls.

Main CodeQL scans confirmed the PyPI injection fixed; reviewed residual transport
reports and duplicate receipt-path alert #34 were individually dismissed with
post-fix evidence. The code-scanning API then reported zero open alerts.

## Native Linux acceptance follow-up

Explicit runs at `dc8a79e0` passed macOS/Windows checks, Clippy, dependency policy,
artifact roundtrip, and [security scanning](https://github.com/goobits/vm/actions/runs/36373626499).
The Linux unit run exposed the remaining dynamically written snapshot executable
fixture. It now follows the crate's immutable fixture pattern to avoid Linux
ETXTBSY races during concurrent fork/exec; related snapshot fixtures were migrated
too. The old assertion did not print its underlying error, so the exact original
errno was not recorded; assertions now retain that diagnostic.

[Onboarding 36373629138](https://github.com/goobits/vm/actions/runs/36373629138)
reached live snapshot create, restore, ownership checks, and native image-tamper
rejection, then failed trying to corrupt a root-owned volume archive. Backup now
creates the archive exclusively as the invoking user with mode 0600 before the
root helper writes it. Existing files cannot be silently overwritten. A disposable
Alpine helper check preserved UID/GID 1000:1000, mode 0600, and exact archived data.
The failed hosted acceptance cleaned its test containers, volumes, images, and
networks; existing host environments were never used by that run.

## Final closeout — 2026-09-28

Final tested source: `2b863feca7313d57f6fc034a4b823b115354e970`, including starting
commit `37658c3d`. All task source changes are integrated on main; subsequent
closeout edits only preserve documentation and remove the completed tracker.

- Local macOS 27.0 (26A428), arm64, Rust/Cargo 1.98.1: all 1,210 workspace tests
  passed, zero failures, 24 existing ignored; all-target/all-feature Clippy with
  warnings denied, formatting, all-feature release build, CLI reference, and
  `git diff --check` passed. No CLI inventory regeneration was required.
- [CI 36375948749](https://github.com/goobits/vm/actions/runs/36375948749) passed
  Linux/macOS/Windows, Clippy, audit, and artifact checks. The explicit dispatch
  selected `package_acceptance=false`: unchanged package code had already passed
  live acceptance in [36373623786](https://github.com/goobits/vm/actions/runs/36373623786)
  at `dc8a79e0`. This option defaults to true for future dispatches.
- [Security 36375951018](https://github.com/goobits/vm/actions/runs/36375951018)
  passed dependency policy and both CodeQL languages, with no analysis errors.
  All reported alerts have individual source-level dispositions in the
  [triage record](code-scanning-triage.md); no query category is disabled.
- [Live onboarding 36375953385](https://github.com/goobits/vm/actions/runs/36375953385)
  passed `VM_ACCEPTANCE_BIN="$(command -v vm)" bash
  scripts/internal/test-provider-workflow-docker.sh` after a source install.
  Host: Ubuntu 24.04.5, linux/amd64, Rust 1.98.1, Docker client/server 28.0.4,
  Compose 2.38.2. Tested binary SHA256:
  `9f269a331244712c32fb17002dbfd4b75f9f9ef25e4acb6b820019ab0bedb2ff`.
  The workflow verified restored data, gzip/none archives, project ownership,
  overwrite/recovery and corruption rejection, environment/fleet operations,
  database routing/backups, tunnels, service plugins, and storage ownership.
  It exited zero at 04:12:38 UTC and cleaned its disposable provider resources.

Final API checks returned no open PRs, issues, CodeQL alerts, Dependabot alerts,
or secret-scanning alerts. The active follow-up tracker is complete and removed.
All six repository workflows are opt-in; their latest runs were explicitly
selected, and no runs were triggered by the intervening documentation pushes.
Release publishing still requires an explicit annotated-tag dispatch and retains
its immutable-publication checks and protected environments.

Temporary review reports, request bodies, and local test logs were removed after
preserving their evidence. The shared Cargo cache remains in place. Existing
`vm-dev`, `sketch-api-dev`, their databases, and package services remain running;
no disposable local containers remain. Remote branches are main, release/5.x,
and two pre-existing branches with unique work; completed task and dependency
branches are gone. Earlier Podman/Tart acceptance and release-publishing limits
remain as recorded in the preceding acceptance documents.
