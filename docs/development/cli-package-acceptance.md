# Docker package acceptance

The complete Docker package workflow passed on 2026-09-26 against commit `37658c3d1100b0d68da1bc867228823aa3e70dd4`
plus the acceptance fixes in this checkout. This document records failures and
recovery without retaining credentials, container inspection payloads, or private
workflow tokens. Host: macOS 27.0 (26A428), arm64; Docker Desktop 4.92.0,
Engine/client 29.8.0, Compose 5.5.1; Rust/Cargo 1.98.1. See the
[host audit](cli-acceptance.md) for runtime installation details.

## Final result

Attempt12 completed with exit0 and `Docker package workflow acceptance passed`.
The host binary was pinned at SHA256
`b6e054dfec68f80f9f8712a8f201470bf53066d94d6c18e86eee60c55f76ed9d`.
The package images were built from the same checkout, including subsequent
provider-only snapshot changes; package code was unchanged after the pinned build.

```sh
cargo build --manifest-path rust/Cargo.toml -p goobits-vm --release --all-features
mkdir -p /tmp/vm-package-acceptance-bin
cp /tmp/vm-rust-target/release/vm /tmp/vm-package-acceptance-bin/vm
PATH="$HOME/.cargo/bin:$HOME/.docker/bin:$PATH" \
  VM_ACCEPTANCE_BIN=/tmp/vm-package-acceptance-bin/vm \
  VM_ACCEPTANCE_KEEP_EVIDENCE=1 \
  bash scripts/internal/test-package-workflow-docker.sh
```

All release, receipt resume, SIGINT exit 130, interruption/retry, consumer review,
running activation, stopped-target deferral, truthful partial failure, tools
refresh/disable, and package-service backup/restore/removal scenarios passed.
The final comparison found all 15 primary container IDs and 30 package-volume
identities unchanged through maintenance. All 10 baseline foreign managed
container IDs were unchanged; concurrently created provider-acceptance resources
were accounted for separately.

Cleanup removed the test environments, networks, volumes (including the inactive
maintenance-profile backup volume), package images, custom images, and derived
images. A process audit found no remaining package activation worker. Earlier
attempts left 38 custom/derived image tags; each was verified against its exact
test project before removal. The standalone Node image reference used by the
maintenance smoke test was also removed. Existing host resources and shared
build caches were preserved. Raw evidence was deleted after this maintained
summary; it is not required to reproduce the workflow.

## Isolation

The workflow uses disposable HOME, projects, package service Compose project,
networks, and volumes. Host inspection found existing managed environments outside
this task. A test-only Docker shim restricts managed-container discovery to the
three exact acceptance runtime names; explicit named Docker operations still use
the real engine. Before publication or global tool mutations, CLI JSON inventory
must contain exactly these three environments. Podman and Tart discovery are
excluded from this Docker-only controller. Detached activation workers inherit
the same restricted discovery. No foreign global mutation was reached before
this isolation guard was added.

The workflow compares primary container IDs and mounted package volume identities
across publication, activation, and maintenance. Cleanup verifies activation-worker
ownership, resumes stopped workers before terminating them, and removes only test
Compose resources and fixture images. It retains shared build caches.

## Diagnostic attempts

All attempts used `bash scripts/internal/test-package-workflow-docker.sh` from the
repository root, with `VM_ACCEPTANCE_BIN=/tmp/vm-rust-target/release/vm` when selected
explicitly. Later runs used `VM_ACCEPTANCE_KEEP_EVIDENCE=1`; local raw evidence is
temporary and is removed after results are captured here.

| Attempt | Result and correction |
| --- | --- |
| 1 | Fixture lacked a declared environment; declared isolated `dev` environments. |
| 2 | Guest CLI was absent after fleet start; fleet and single start now share runtime reconciliation. |
| 3 | Host PATH omitted Docker; corrected runner invocation PATH, no provider resources created. |
| 4 | Harness used removed checkout-ID `packages show` behavior; assert durable checkout state instead. |
| 5 | Editing the active Bash runner shifted its read position; harness error only. Runner and sourced scripts are frozen throughout subsequent invocations. |
| 6 | Guest Node toolchain download timed out during provisioning. Pinned Node22 fixture now includes real NVM and pnpm; production toolchain reconciliation reports current twice with Docker networking disabled. |
| 7 | Consumer upgrade failed fetching a clean npm tarball URL with HTTP401. Job helper previously embedded credentials only in registry URL; scoped mode 600 npmrc now authenticates metadata tarball URLs. A live `npm pack` check and the next complete consumer review passed. |
| 8 | Second stopped-target start activated stale 1.0.0 after 1.1.0 was already active. Deferred selection now chooses newest target activation across all states before filtering deferred work; a unit regression covers superseded deferred releases. Canonical receipt resume also now uses durable checkout kind to avoid an erroneous local checkout cleanup warning. |
| 9 | All release, activation, review, and selection scenarios passed, including repeated stopped-target start without downgrade and clean canonical receipt resume. Backup creation found a harness-only flag error; corrected to positional NAME after checking built help for every maintenance command. |
| 10 | All release/tool scenarios passed. Backup creation could not read private published artifacts because maintenance dropped every capability. Networkless maintenance now receives only DAC_OVERRIDE, CHOWN, and FOWNER; an isolated live check preserves private content and numeric ownership/mode through archive restore. |
| 11 | All maintenance mutations passed, including private-data backup, corruption rejection, restored marker, automatic naming, and removal. Final empty-list harness comparison needed to allow the existing engine readiness status line; changed to require the exact empty-list result line. Final post-maintenance identity assertions subsequently passed in attempt 12. |

Attempt8 used host binary SHA256
`8bb8be321ead6ca9c79aad888a3a45a266e03a7c346863198f91ca54f98b9884`
and the corrected package-job npm helper. It passed source-only language
publication, consumer review branch submission with unchanged main pin, dependency
restoration failure/retry, background release and receipt resume, actual SIGINT
exit130 with receipt diagnostics, controller restart, truthful running-target
partial failure and stopped-target deferral, repair/resume, running activation,
first stopped-target activation, repeat release idempotency, and isolated global
tool enable/disable. Package backup maintenance had not yet run when it failed.

The restart test restarts the disposable package controller by default. It does
not restart the shared host Docker daemon. The workflow supports an explicitly
configured daemon restart command for a dedicated host.

The maintenance command was also extracted unchanged from embedded Compose and
run with `docker run --rm --user 0:0 --cap-drop ALL --cap-add DAC_OVERRIDE
--cap-add CHOWN --cap-add FOWNER --read-only --network none`, using only disposable
tmpfs volumes. A 256 KiB random artifact overflowed 128 KiB backup storage; the failed
create removed both temporary/final output, and the same name succeeded after
removing the oversized fixture. A separate actual tar roundtrip preserved a
mode 600 artifact owned 10001:10001 and its content. These checks exercised failure
recovery and least-privilege backup behavior without package service data.

## Scope and limitations

Acceptance covers the private package service and the fixture manifest workflows;
public registry publication was outside this tracker. Consumer versions require
registration after reviewed updates; automatic merge detection and monorepo-wide
consumer discovery are not implemented. Language validation follows npm and
pip/pytest conventions. pnpm/Yarn editable installs, Yarn PnP, Poetry consumer
updates, and dependencies outside root manifests require separate acceptance.
Python releases require static, stable semantic versions; dynamic metadata and
broader Python version schemes are outside the release contract.
