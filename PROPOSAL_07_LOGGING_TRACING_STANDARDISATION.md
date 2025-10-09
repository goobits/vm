# Proposal: Standardise Logging & Tracing Across Crates

**Status**: Draft  
**Owner**: Platform Core  
**Target Release**: 2.3.0

---

## Motivation

Today the workspace mixes `println!`, `vm_println!`, `log`, and `tracing` macros inconsistently. Some providers emit plain stdout while others use structured spans. To present a polished, professional CLI:
- Logs should follow a consistent format per severity (info/warn/error)
- Commands should attach tracing spans for lifecycle steps
- Providers should be able to plug into central tracing configuration

---

## Objectives

1. **Adopt `tracing` everywhere**: Replace ad-hoc logging with `tracing::{info!, warn!, error!, span!}`.
2. **Bridge macros**: Update `vm_println!` / `vm_error!` to delegate to tracing + structured output.
3. **Configure subscribers**: Provide a single `tracing_subscriber` setup (JSON in CI, colourful text locally).
4. **Guidelines**: Document logging patterns (when to emit spans vs. events) in `CONTRIBUTING.md`.

---

## Implementation Outline (3 PRs)

1. **Infrastructure**: Add `tracing_subscriber` initialisation in `vm` and helper crate for other binaries; update macros to use tracing.
2. **Command modules**: Sweep CLI handlers (`vm_ops`, `config_ops`, package server) replacing `println!`/`log!` with tracing events.
3. **Providers**: Update provider crates (docker, vagrant, tart) to emit spans around lifecycle steps and consistent events during errors.

Each PR keeps behaviour identical for end users (stdout messages unchanged) but ensures structured logs are available for power users.

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Double-printing (stdout + tracing) | Wrap macros so only tracing emits; CLI formatting uses subscriber layers. |
| Performance impact | Tracing overhead is minimal; avoid heavy fields in hot loops. |
| Third-party logging (e.g. duct/command output) | Capture and redirect external process logs via tracing adapters. |

---

## Success Criteria

- All crates depend on `tracing` (no direct `log` usage).
- CLI output identical to current behaviour in default subscriber.
- Structured logs available when `VM_JSON_LOGS=1`.
- Documentation updated with logging guidelines.

---

## Follow-ups

- Integrate with `tracing-error` to capture backtraces.
- Expose machine-readable logs for automation (`vm --json`).
