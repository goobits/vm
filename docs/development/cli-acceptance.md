# CLI acceptance log

## 2026-09-25 host audit

Tested starting revision: `37658c3d` (clean working tree).
Host: macOS 27.0 (26A428), arm64.

- `git merge-base --is-ancestor 37658c3d HEAD`: passed.
- `docker version` and `docker info`: passed outside the filesystem sandbox.
  Docker Desktop 4.92.0 (240144), Engine/client 29.8.0, Linux arm64 daemon.
- `docker compose version`: 5.5.1.
- `podman version` / `podman info`: command not found.
- `tart --version` / `tart list`: command not found.
  Neither binary exists in `/opt/homebrew/bin` or `/usr/local/bin`.
- Rust tools are installed under `~/.cargo/bin`; commands add it to `PATH`.

Podman and Tart were initially unavailable. On 2026-09-26, Homebrew 7.0.6
installed Podman 6.1.2 and Tart 2.38.0 (with Softnet 0.23.0), using
`brew install podman` and `brew install openai/tools/tart` (the
[official Tart quick start](https://github.com/openai/tart/blob/main/docs/quick-start.md)
names the tap). The specific Softnet
formula required `brew trust --formula openai/tools/softnet`. Homebrew auto-update
and install cleanup were disabled; no host DHCP settings were changed.
Acceptance uses disposable project names and isolated configuration homes; existing
Docker workloads are preserved. Installation alone is not acceptance.

## Execution

`cargo build --manifest-path rust/Cargo.toml -p goobits-vm --release --all-features`
passed with Rust/Cargo 1.98.1 and was repeated after the acceptance fixes.
The binary is `/tmp/vm-rust-target/release/vm`, from repository Cargo configuration.

The package workflow was invoked from the repository root with
`bash scripts/internal/test-package-workflow-docker.sh`. Two failed attempts
preserved evidence before resource teardown:

- `vm-package-acceptance.IQTn10`: obsolete fixtures lacked a declared environment.
- `vm-package-acceptance.7EGEJA`: `start` did not install the managed guest CLI.

These failed attempts initially preserved raw evidence under the host temporary
directory. Completed attempt archives, logs, and helpers were removed after review;
the maintained [package evidence](cli-package-acceptance.md) records their results.
They are failed attempts, not acceptance.

[Provider acceptance](cli-provider-acceptance.md) records Docker diagnostics and
snapshot work. [Podman acceptance](cli-podman-acceptance.md) also passed;
Tart snapshot and broader acceptance also passed; all provider requirements are complete.

Workspace execution exposed a symlink-sensitive macOS ownership test fixture,
untruthful successful partial storage inventory, and an obsolete integration test
for the removed `vm temp` command. The ownership fixture now matches the
planner's canonical path contract. Storage errors use one failed JSON envelope
with operational exit 1. The obsolete test was removed; current declared
environment lifecycle is exercised by the live provider workflow. No legacy
command compatibility was introduced.

## Workspace validation

The following gates passed on 2026-09-26 after the current fixes:

```sh
cargo test --manifest-path rust/Cargo.toml --workspace --all-features
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --all-features -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
bash scripts/internal/check-cli-reference.sh
git diff --check
```

Workspace result: 1,191 passed, zero failed, 24 ignored across 59 test targets
(including documentation tests). Provider-mutating ignored tests were not enabled;
the isolated live workflows provide acceptance instead. Loopback HTTP test fixtures
require running outside the filesystem sandbox. Generated command inventory
matched without regeneration; no parser changes were made.

The validated tracked implementation diff against `37658c3d` (`rust` and `configs`) had
SHA-256 `3042e802ee5f4251d2bd8f1aaef6923ca211cb1969564b634609a5fd69274c10`.
The corresponding release binary built successfully with SHA-256
`522b26c0e9efc4d25e8cad9e6ca277175a8a927cebfdc78f63169eeca48df605`.
Provider-specific evidence records the pinned binary used by each live run.

One parallel suite run exposed a timing-sensitive lock test: a concurrently
forked child can briefly retain a lock descriptor until exec. The test still
requires immediate exclusion while held and now permits a bounded one-second
release interval. Production locking is unchanged; the subsequent full suite passed.

Shell syntax checks passed for all three acceptance runners and all package scenario
scripts. The Tart builder's existing two-guest test caught a macOS `mktemp`
template error; placing the random suffix last fixes it without changing the
provider contract. That test now passes, but is not live Tart acceptance.

The release-binary Docker provider workflow passed; its exact revision, binary
hash, coverage, and cleanup audit are in [provider evidence](cli-provider-acceptance.md).
The complete package workflow passed, including exact identity comparisons and
cleanup; the maintained
[package evidence](cli-package-acceptance.md) records individual attempts,
regressions, isolation, and verified coverage.

## Closure

All five originally unchecked checklist items passed: the Docker package workflow,
package evidence/failure resolution, cross-provider snapshots, broader provider
acceptance, and tracker closure. The completed CLI and package proposals were
removed after their evidence and scope limitations were preserved here and in the
linked provider/package logs. No parser change required inventory regeneration.

The final Docker audit preserved all 10 original containers, 22 volumes, and nine
networks. Package maintenance retained 15 primary container identities and 30
package-volume identities. Test-owned Docker resources, the disposable Podman
machine/connections, Tart VMs and isolated image cache, and temporary logs,
archives, helpers, and baseline files were removed. Shared build caches and the
built release binary remain available; installed runtimes are retained.

## Snapshot review follow-up — 2026-09-26

Review after `73d11cb0` found two issues beyond the scenarios exercised above.
Native snapshots now store the captured runtime receipt fingerprint and restore
that fingerprint with the disk identity. Restoring configuration A over a runtime
recreated with B therefore reports drift against B. Missing or invalid capture
fingerprints are rejected before native import or restore; no fingerprint is
inferred from the restore target. The Tart runner includes this A/B regression.

Container capture now removes its exact temporary image tag after saving the
archive and on failed capture, including partial commits. Cleanup failure keeps
the original failure context and reports the exact removal command. It never
forces deletion or removes unrelated image tags.

Local verification covers the snapshot, provider, and CLI suites (612 passing
tests, 20 ignored), all-targets Clippy, formatting, and the Tart runner's shell
syntax. Tests cover capture and cleanup failures, repeated unique captures,
missing native fingerprints, replaced source disks, and A/B receipt restoration.
These follow-up fixes were tested without live runtimes; the host acceptance
results above describe the preceding revision.
