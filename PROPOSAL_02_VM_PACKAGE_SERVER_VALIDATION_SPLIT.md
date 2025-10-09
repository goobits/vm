# Proposal: Split `vm-package-server/src/validation.rs`

**Status**: Draft  
**Owner**: Package Server Team  
**Target Release**: 2.2.0

---

## Problem

`validation.rs` has grown to **1,146 LOC** with four distinct responsibilities:

- Input size limits & thresholds
- Path and filename validation / traversal protection
- Shell/Docker escaping utilities
- Package manifest checks (npm, PyPI, cargo)

The single mega-module causes:
- High cognitive load when auditing security-sensitive helpers
- Frequent merge conflicts between feature teams
- Test brittleness (one change touches every helper)
- Difficulties sharing logic with new subcommands (e.g. package removal)

---

## Proposed Structure

Create a `validation/` module directory:
```
vm-package-server/src/validation/
├── mod.rs            // Re-exports + shared constants
├── limits.rs         // MAX_* constants, streaming thresholds
├── paths.rs          // Path & filename validation helpers
├── shell.rs          // Shell escaping / command sanitizing
├── docker.rs         // Docker name/label validation
├── manifests.rs      // Package manifest/schema checks
└── tests/            // Unit tests per domain (optional)
```

### Module Responsibilities
- `limits.rs` — pure data & simple getters.
- `paths.rs` — `validate_safe_path`, `validate_filename`, depth checks.
- `shell.rs` — `escape_shell_arg`, argument quoting, env sanitation.
- `docker.rs` — container/image name validation, port checks.
- `manifests.rs` — npm/package manifest validation, JSON schema helpers.
- `mod.rs` — `pub use` façade + common error helpers.

Each module stays < 250 LOC, making targeted audits straightforward.

---

## Implementation Plan (3 PRs)

| PR | Scope | Key Moves | Est. LOC | Effort |
|----|-------|-----------|----------|--------|
| 1 | Limits + Paths | Create `validation/` dir, move constants & path helpers | ~350 moved | 0.5 day |
| 2 | Shell + Docker | Split escaping & docker helpers, update call sites | ~400 moved | 0.5 day |
| 3 | Manifests & Cleanup | Move manifest checks, add module docs/tests | ~400 moved | 0.5 day |

Each PR keeps tests green (`cargo test -p vm-package-server`).

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Accidental API change | Maintain identical function signatures & `pub use` in `mod.rs`. |
| Missed re-export | Add `cargo doc` step to ensure items remain visible. |
| Test gaps during migration | Run package-server unit + integration tests after every PR. |

---

## Success Criteria

- All validation helpers imported via `validation::*` continue to compile without call-site changes.
- `validation.rs` deleted, new modules <= 250 LOC each.
- Security audit checklists reference discrete modules.
- No new clippy warnings.

---

## Follow-ups (Optional)

- Add fuzz tests for `paths` & `shell` modules.
- Document validation responsibilities in `docs/package-server.md`.
