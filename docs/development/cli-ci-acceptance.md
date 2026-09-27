# CLI cross-platform CI acceptance

The CI follow-up to the [live provider acceptance](cli-acceptance.md) passed on
2026-09-27 at revision `69c73ce99f31f32b107a022fe13c286526b43b30`.
This closes the Windows CI and native Linux package acceptance failures tracked
by issues [#65](https://github.com/goobits/vm/issues/65) and
[#87](https://github.com/goobits/vm/issues/87), through
[PR #88](https://github.com/goobits/vm/pull/88).

## Results

- [Continuous Integration](https://github.com/goobits/vm/actions/runs/36302183936):
  Linux, macOS, and Windows unit/CLI/integration tests passed; Linux shell
  regressions, Clippy, security audit, and the complete Docker package workflow
  passed.
- [Code coverage](https://github.com/goobits/vm/actions/runs/36302183939) and
  [security scan](https://github.com/goobits/vm/actions/runs/36302183938) passed.
- Local workspace validation: 1,200 passed, zero failed, 24 existing ignored.
  Clippy with warnings denied, formatting, generated CLI reference, and whitespace
  checks passed. No parser inventory regeneration was needed.
- Local release build SHA256:
  `1cc5f581e13057831eea2c503a4f14583cc508c85ba1491b47d2ed236d4b8d39`.

```sh
cargo build --manifest-path rust/Cargo.toml -p goobits-vm --release --all-features
cargo test --manifest-path rust/Cargo.toml --workspace --all-features
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --all-features -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
bash scripts/internal/check-cli-reference.sh
git diff --check
```

## Docker coverage and isolation

Native Linux runner: x86_64, kernel 6.17.0-1022-azure; Docker Engine/client
28.0.4, Compose 2.38.2; Rust/Cargo 1.98.1. The workflow records these versions
before acceptance.

The native Linux job ran:

```sh
VM_ACCEPTANCE_BIN="$PWD/rust/target/release/vm" \
  VM_ACCEPTANCE_DOCKER_RESTART_COMMAND='sudo systemctl restart docker' \
  bash scripts/internal/test-package-workflow-docker.sh
```

It verified publication, consumer review, background release and receipt resume,
SIGINT exit 130, interrupted activation/retry, truthful partial failures, running
activation, stopped-target deferral, tools disable/refresh, and package-service
backup/restore/removal. Primary container and package-volume identities remained
unchanged. After the daemon restart the harness restarted only test-owned
containers that had previously been running; the stopped target remained stopped.

The complete workflow also passed locally using the release binary built at
`db28d71f90a85d8223cc543655428547c18302da`. That local run restarted the package
controller, while CI covered the Docker daemon restart. Local host: macOS 27.0
(26A428), arm64; Docker Engine/client 29.8.0, Compose 5.5.1; Rust/Cargo 1.98.1.
The final source revision was rebuilt and received the full local validation above.

Disposable acceptance resources were removed. The existing `vm-dev` and
`sketch-api-dev` container identities were unchanged and their installed skills
remained present. Shared runtime/build caches and the real package appliance were
preserved. Original Podman/Tart acceptance remains recorded in the linked live
provider evidence; this follow-up did not rerun those providers.

## Corrections verified

- Guest Unix paths are validated independently of Windows host path semantics;
  mount declarations preserve Windows drive letters.
- Windows file locking uses the native fs2 contention error. Atomic-update tests
  close temporary handles, and durable writes use writable handles.
- Windows profile overrides and isolated AppData fixtures prevent tests from
  sharing the runner's real configuration. POSIX guest shell fixtures run on Unix.
- Compose secret files are readable through explicit container mounts while their
  host parent directory remains private; controller write access is retained.
- Linux fixture users match host ownership, and disposable clone helpers trust
  only their explicitly mounted Git fixture paths.
- Immutable executable fixtures avoid Linux executable-write races. Snapshot
  fixtures passed 20 consecutive parallel suites (800 tests). Lifecycle inventory
  errors now propagate instead of being reported as missing containers.

Failed attempts were inspected before correction. Raw logs and temporary helpers
are disposable; this document and the linked CI runs retain the acceptance evidence.

## Mount-source review follow-up

Review of the Windows guest-path change found that host drive roots could pass the
existing Unix-only source guard. The follow-up rejects drive and UNC share roots,
device namespaces, and Windows system directories after canonicalization, including
verbatim prefixes and configured system-directory locations. Ordinary project
directories remain mountable.

The follow-up passed 1,201 local workspace tests (24 existing ignored), Clippy,
formatting, CLI reference checks, and the release build. Tests cover Windows path
spellings on every host and actual canonical volume-root rejection on Windows CI.
The follow-up binary SHA256 is
`f1b456f0b55f5ec32a14362e83d2aea3c0123e78c7c4b202f8b4a3d55237fb4d`.
Final cross-platform results are retained with [PR #88's checks](https://github.com/goobits/vm/pull/88/checks).
