# Proposal: Modularise `vm-config/src/config_ops.rs`

**Status**: Draft  
**Owner**: Config Platform  
**Target Release**: 2.2.0

---

## Problem

`config_ops.rs` (923 LOC) implements every `vm config` subcommand: get/set/unset, preset application, dry-run messaging, port placeholder resolution, and YAML persistence. This monolith:
- Increases review time—unrelated features collide in the same file
- Hinders focused unit tests (single module-level test suite)
- Obscures error handling differences between commands
- Encourages copy/paste when adding new config operations

---

## Proposed Layout

```
vm-config/src/config_ops/
├── mod.rs                // Facade, shared errors
├── get.rs                // `config get` logic + formatting
├── set.rs                // `config set` + validation helpers
├── unset.rs              // `config unset`, cleanup utilities
├── preset.rs             // preset discovery + application
├── port_placeholders.rs  // ${port.X} interpolation helpers
├── io.rs                 // file read/write, dry-run support
└── tests/ (optional)     // command-specific unit tests
```

`mod.rs` re-exports existing `config_ops::*` API so CLI callers remain unchanged.

---

## Implementation Steps (4 PRs)

1. **Scaffold modules** – create directory, move shared types/aliases into `mod.rs`, migrate port placeholder logic (`port_placeholders.rs`).
2. **Get/Set split** – move `handle_get`/`handle_set` and supporting functions into dedicated files; point `mod.rs` re-exports to new modules.
3. **Unset/Preset split** – relocate unset + preset flows, ensuring preset detector imports stay localised.
4. **IO consolidation & cleanup** – move file persistence helpers into `io.rs`, delete legacy `config_ops.rs`.

Each PR compiles and passes `cargo test -p vm-config`.

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Loss of shared helpers | Keep cross-cutting utilities in `io.rs` / `port_placeholders.rs` referenced with `pub(super)` scope. |
| CLI breakage | Maintain `pub use` of existing functions in `mod.rs`; add temporary assertions to ensure exports match. |
| Increased code duplication | During migration, place reused logic in `io.rs` before splitting commands. |

---

## Success Metrics

- All references to `config_ops::*` resolve via new `mod.rs` exports without code changes.
- No module in `config_ops/` exceeds 250 LOC.
- Command-specific unit tests live alongside the split modules.
- `cargo clippy -p vm-config` yields zero new warnings.

---

## Nice-to-haves (Post split)

- Add snapshot tests for `config get --json` output.
- Document `config_ops` architecture in `docs/config-cli.md`.
- Expose shared IO helpers for other tooling crates.
