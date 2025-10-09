# Proposal: Split `vm-config/src/cli/mod.rs`

**Status**: Draft  
**Owner**: Config Platform  
**Target Release**: 2.3.0

---

## Problem

`cli/mod.rs` (735 LOC) defines the entire clap interface for the config tool:
- Top-level parser (`Args`),
- Subcommand enums (`ConfigCommand`, `PresetSubcommand`, etc.),
- Flag definitions, validation, and usage strings.

Consequences:
- Adding a new subcommand creates large diffs and frequent merge conflicts.
- Hard to reason about required/optional flags per subcommand.
- No clear home for future flags (e.g. config lint, schema inspect).

---

## Proposal

Adopt the same structure as the `vm` CLI: break subcommands into individual modules.

```
vm-config/src/cli/
├── mod.rs              // Top-level Args + dispatcher
├── config_cmd.rs       // `config` subcommand definitions
├── preset_cmd.rs       // preset-specific options/enums
├── lint_cmd.rs         // (future) lint flags
├── shared.rs           // common clap value parsers/helpers
└── tests.rs (optional) // parser smoke tests
```

`mod.rs` keeps the top-level `Args` struct but defers subcommand clap annotations to dedicated files using `#[derive(Subcommand)]` + `pub use`.

---

## Implementation Outline (2 PRs)

1. **Refactor existing subcommands** – move `ConfigCommand` and preset enums into `config_cmd.rs` / `preset_cmd.rs`, expose via `pub use`. Ensure `Args` references moved types.
2. **Shared helpers + cleanup** – extract clap validators (`parse_key_value`, path resolvers) into `shared.rs`, delete monolithic `cli/mod.rs` body.

Each PR keeps `cargo test -p vm-config` green and ensures `cargo run --bin vm-config -- --help` unchanged.

---

## Risks & Mitigation

| Risk | Mitigation |
|------|------------|
| Clap attribute drift | Keep subcommand definitions `#[path = "config_cmd.rs"]` ensures attributes remain adjacent to types. |
| Documentation mismatch | Regenerate CLI docs/help snapshots post-refactor. |
| Future flag placement unclear | Document module layout in new `cli/README.md`. |

---

## Success Criteria

- `--help` output identical before/after split.
- Each subcommand module < 250 LOC.
- New helper module covers all shared parsers (no duplication).
- No additional clap warnings at runtime.

---

## Follow-up Ideas

- Add parser unit tests similar to `vm/tests/cli_args.rs` once split.
- Support plugin subcommands by adding optional module hook.
