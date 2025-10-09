# Proposal: Split `vm/tests/vm_operations_integration_tests.rs`

**Status**: Draft  
**Owner**: Dev Experience  
**Target Release**: 2.3.0

---

## Problem

`vm_operations_integration_tests.rs` is **941 lines** of mixed integration scenarios:
- VM create/destroy lifecycle
- Multi-instance flows
- Docker/Vagrant/Tart provider assertions
- Service registration checks
- Utility helpers for temp dirs & Docker cleanup

This single mega-test file leads to:
- Slow iteration—editing any scenario runs the whole suite
- Merge conflicts when teams touch unrelated providers
- Difficult triage (failing test output references massive file/line numbers)
- Hidden duplication of fixtures and helper logic

---

## Proposed Layout

```
vm/tests/vm_ops/
├── mod.rs                          // test harness + shared fixtures
├── create_destroy_tests.rs         // create/destroy + force scenarios
├── multi_instance_tests.rs         // multi-instance provider flows
├── provider_parity_tests.rs        // provider-specific assertions
├── service_lifecycle_tests.rs      // service registration/auto-start
└── helpers.rs                      // shared fixtures (TempDir, binary lookup)
```

`mod.rs` sets up shared environment variables (`VM_TEST_MODE`, fake HOME) and re-exports helper fixtures.

---

## Implementation Plan (2 PRs)

1. **Scaffold & migrate helpers** – create `vm_ops/` directory, move common fixtures into `helpers.rs`, update imports (`use crate::vm_ops::helpers::*`). Keep all tests compiling.
2. **Split scenarios** – move tests into domain-specific files, re-export `mod.rs` so `cargo test vm_ops` still runs by default. Remove original file after final PR.

---

## Risks & Mitigation

| Risk | Mitigation |
|------|------------|
| Hidden coupling between tests | Identify shared state (mutexes, docker cleanup) and centralise in `helpers.rs`. |
| Increased compile time | Modules still under `#[cfg(test)]`—Rust will only compile what’s used. |
| Lost test ordering | Ensure each module uses local mutex or unique container names to keep isolation. |

---

## Success Metrics

- Integration tests run with `cargo test -p vm --test vm_ops` identical to today.
- No file in `vm/tests/vm_ops/` exceeds 250 LOC.
- Shared fixtures consolidated in `helpers.rs`, removed from individual test files.
- Parallel test runs remain stable on CI (document mutex usage).

---

## Follow-ups

- Add provider-specific feature flags (e.g. skip Tart tests on non-macOS).
- Introduce snapshot assertions for CLI output once tests are isolated.
