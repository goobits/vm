# Proposal: CLI Test Coverage Hardening

**Status**: Draft  
**Owner**: Dev Experience  
**Target Release**: 2.2.0

---

## TL;DR

Current CLI tests hit only a handful of commands (config, temp, limited pkg/auth flows). Most of the `vm` surface (`init`, `doctor`, `list`, `status`, `ssh`, `exec`, `logs`, etc.) has zero automated coverage, and several suites rely on brittle binary paths. This proposal adds lightweight unit tests for argument parsing, expands integration coverage to the highest‑impact commands, and stabilises fixtures so contributors can run the suite without Docker or prebuilt binaries.

---

## Problem

- > 20 CLI entry points; < 25 % have tests. Regressions in `vm doctor`, `vm init`, or `vm list/status` go unnoticed.
- Integration fixtures shell out to `/workspace/.build/target/debug/vm`; on fresh checkouts this path is missing, so tests skip or fail.
- Docker-dependent tests are slow and flaky; the suite offers no fast path for everyday development.

---

## Goals (Phase 1)

1. **Parser Safety Nets** – add `clap` unit tests for core commands (`init`, `create`, `start`, `temp`, `pkg`, `auth`, `plugin`) to ensure flags/args stay valid.
2. **Critical Flow Coverage** – integration tests for:
   - `vm init` + `vm validate`
   - `vm doctor` happy-path (mocked dependencies)
   - `vm list` / `vm status` with mock provider results
   - `vm ssh` / `vm exec -- dry-run` using an in-memory shim
3. **Fixture Cleanup** – shared helper that resolves the binary via `CARGO_BIN_EXE_vm` (with graceful skip) and a mock provider harness to avoid Docker for default runs.

---

## Out of Scope (Phase 1)

- Full end-to-end Docker provisioning (already guarded by `VM_INTEGRATION_TESTS`).
- Windows-specific harness work.
- Telemetry around CLI usage.

---

## Implementation Sketch

1. `rust/vm`  
   - new `tests/cli_args.rs` exercising `Args::parse_from` / `Command` variants for the top-level and high-traffic subcommands.  
   - clap-focused unit tests live beside the real definitions (`rust/vm/src/cli/mod.rs`), keeping the `vm-cli` utility crate unchanged for Phase 1.
2. `rust/vm/tests`  
   - shared `fixture::binary_path()` helper; update existing suites to use it.  
   - add `doctor_cli_tests.rs`, `status_cli_tests.rs`, `ssh_exec_cli_tests.rs`.
   - introduce a mock provider (feature-gated) that returns canned responses for list/status/exec.
3. `rust/vm-provider/src/mock.rs`  
   - extend `MockProvider` with configurable `instances` and `status_report` so integration tests can assert on `list`/`status` output without Docker.
4. CI configuration  
   - run fast CLI tests in default job; keep Docker-heavy tests behind `VM_INTEGRATION_TESTS=1`.

Estimated effort: 3–4 engineering days.

---

## Example Tests & Fixtures

### Parser Safety Net (`rust/vm/tests/cli_args.rs`)
```rust
#[test]
fn create_command_parses_flags() {
    use clap::Parser;
    let args = Args::parse_from(["vm", "create", "--force", "--instance", "test"]);
    match args.command {
        Command::Create { force, instance, .. } => {
            assert!(force);
            assert_eq!(instance, Some("test".to_string()));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}
```

### Mocked Status Flow (`rust/vm/tests/status_cli_tests.rs`)
```rust
#[test]
fn status_uses_mock_provider() {
    let provider = MockProvider::with_status(VmStatusReport {
        name: "test-vm".into(),
        is_running: true,
        ..Default::default()
    });

    let output = run_status_with_provider(provider).unwrap();
    assert!(output.contains("test-vm"));
    assert!(output.contains("running"));
}
```

### Binary Resolution Helper
```rust
pub fn binary_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_vm") {
        return Ok(PathBuf::from(path));
    }
    let fallback = PathBuf::from("/workspace/.build/target/debug/vm");
    if fallback.exists() {
        return Ok(fallback);
    }
    Err("vm binary not found – run `cargo build` or set CARGO_BIN_EXE_vm".into())
}
```

Fixtures will call this helper; tests skip gracefully when neither path exists.

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Mock provider diverges from real provider behaviour | Keep fixtures simple; reuse existing traits from `vm_provider::mock`. |
| Additional test time slows CI | Unit tests are fast; mock-based integrations avoid Docker. |
| Contributors miss integration suite | Document `VM_INTEGRATION_TESTS` flag in README. |

---

## Success Metrics

- New unit suite exercises ≥80 % of `Command` variants.
- CLI integration tests run (and pass) locally with just `cargo test -p vm`.
- No more hard-coded `.build/` binary paths in test fixtures.

---

## Next Steps

1. Approve scope.  
2. Implement Phase 1 tests + fixtures.  
3. Update developer docs (testing section).  
4. Re-evaluate coverage; plan Phase 2 (multi-provider, Windows) if needed.
