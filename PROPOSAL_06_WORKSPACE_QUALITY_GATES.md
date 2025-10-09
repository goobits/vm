# Proposal: Workspace Quality Gates (fmt, clippy, deny)

**Status**: Draft  
**Owner**: DevOps/Tooling  
**Target Release**: 2.2.0

---

## Motivation

The workspace spans multiple crates (`vm`, `vm-config`, `vm-provider`, package server, etc.). Contributors sometimes miss running `cargo fmt` or `clippy`, leading to noisy review cycles. We also lack a unified dependency audit. Professionalising the repo means:
- Style and lint errors are caught before review
- Third-party dependencies are vetted (`cargo deny`)
- CI communicates failures consistently

---

## Proposal

Introduce a workspace “quality gates” pipeline that runs locally and in CI:

1. **Formatting**: `cargo fmt --all --check`
2. **Clippy lints**: `cargo clippy --workspace --all-targets -- -D warnings`
3. **Dependency audit**: `cargo deny check licenses bans sources`
4. **Doc build smoke test**: `cargo doc --workspace --no-deps` (optional)

Add a single `just`/`make` task (`just quality-gates`) that developers can run locally.

---

## Implementation Plan

1. Add a `justfile` (or extend existing) with `quality-gates` recipe.
2. Create CI workflow (`.github/workflows/quality_gates.yml`) executing the four commands.
3. Configure lint allowlists where necessary (e.g. clippy exceptions in `clippy.toml`).
4. Document workflow in `CONTRIBUTING.md`.

Effort: ~1 day.

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Clippy introduces regressions | Start with warning-as-error; selectively allow lints per crate. |
| Cargo deny false positives | Add `deny.toml` exceptions (e.g. for GPL-licensed dev-tools). |
| Increased CI time | Run formatting + clippy in parallel jobs; dependency audit is fast (<1 min). |

---

## Success Criteria

- CI fails on unformatted code, clippy warnings, or banned dependencies.
- Contributors adopt `just quality-gates` before pushing PRs.
- No false negatives after initial allowlist tuning.
- Documented in onboarding materials.

---

## Future Enhancements

- Add `cargo udeps` (unused dependency) scan in nightly builds.
- Cache clippy artifacts per crate to speed up CI.
